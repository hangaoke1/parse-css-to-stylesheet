use std::{rc::Rc, cell::RefCell, convert::Infallible, collections::HashMap, hash::Hash};

use lightningcss::{stylesheet::{StyleSheet, ParserOptions, PrinterOptions}, visitor::{Visit, Visitor, VisitTypes}, visit_types, rules::CssRule, properties::Property, declaration::DeclarationBlock, traits::ToCss};

use crate::{style_parser::StyleDeclaration, utils::to_camel_case, style_propetries::style_value_type::StyleValueType};

use super::parse_style_properties::parse_style_properties;

pub type StyleValue = HashMap<String, StyleValueType>;

pub struct StyleData<'i> {
  pub style_record: Rc<RefCell<HashMap<String, Vec<(String, Property<'i>)>>>>,
  pub all_style: Rc<RefCell<HashMap<String, StyleValue>>>,
}

struct StyleVisitor<'i> {
  all_style: Rc<RefCell<Vec<(String, Vec<StyleDeclaration<'i>>)>>>,
}

impl<'i> StyleVisitor<'i> {
  pub fn new(
    all_style: Rc<RefCell<Vec<(String, Vec<StyleDeclaration<'i>>)>>>,
  ) -> Self {
    StyleVisitor {
      all_style
    }
  }
}

// 收集所有的样式到 all_style 中，以元祖的形式存在 (selector, vec[declaration1, declaration2, ...])
impl<'i> Visitor<'i> for StyleVisitor<'i> {
  type Error = Infallible;
  const TYPES: VisitTypes = visit_types!(RULES);
  fn visit_rule(&mut self, rule: &mut CssRule<'i>) -> Result<(), Self::Error> {
    match rule {
      CssRule::Style(style) => {
        let selectors_str = style.selectors.to_string();
        let selectors = selectors_str.split(",").collect::<Vec<&str>>();
        for index in 0..selectors.len() {
          let selector = selectors[index].trim().replace(".", "");
          let mut all_style = self.all_style.borrow_mut();
          let decorations = all_style.iter_mut().find(|(id, _)| id == &selector);
          if let Some((_, declarations)) = decorations {
            declarations.push(StyleDeclaration {
              specificity: style.selectors.0.get(index).unwrap().specificity(),
              declaration: style.declarations.clone(),
            });
          } else {
            all_style.push((
              selector.clone(),
              vec![StyleDeclaration {
                specificity: style.selectors.0.get(index).unwrap().specificity(),
                declaration: style.declarations.clone(),
              }],
            ));
          }
        }
      }
      _ => {}
    }
    Ok(())
  }
}

pub struct RNStyleParser<'i> {
  pub all_style: Rc<RefCell<Vec<(String, Vec<StyleDeclaration<'i>>)>>>
}

impl<'i> RNStyleParser<'i> {
  pub fn new() -> Self {
    RNStyleParser {
      all_style: Rc::new(RefCell::new(vec![]))
    }
  }

  pub fn parse(&mut self, css: &'i str) {
    let mut stylesheet = StyleSheet::parse(css, ParserOptions::default()).expect("解析样式失败");
    let mut style_visitor = StyleVisitor::new(Rc::clone(&self.all_style));
    stylesheet.visit(&mut style_visitor).unwrap();
  }

  pub fn calc(&self) -> HashMap<String, Vec<StyleValueType>> {
    // 遍历 style_record，计算每个节点的最终样式
    let mut all_style = self.all_style.borrow_mut();
    let mut style_record = HashMap::new();
    let mut final_all_style = self.calc_style_record(&mut all_style);
    
    // final_all_style 转换为驼峰命名
    let mut final_all_style = final_all_style.iter_mut().map(|(selector, style_value)| {
      // calc_style_record 已经处理掉了所有important_declarations，所以这里不需要再处理
      let properties = style_value.declaration.declarations.iter().map(|property| {
        (
          to_camel_case(property.property_id().to_css_string(PrinterOptions::default()).unwrap().as_str(), false),
          property.clone()
        )
      })
      .collect::<Vec<(_, _)>>();
      (selector.to_owned(), properties)
    })
    .collect::<Vec<(_, _)>>();

    for (selector, style_value) in final_all_style.iter_mut() {
      let declarations = style_record.entry(selector.to_owned()).or_insert(vec![]);
      declarations.push(style_value.clone());
    }

    let final_style_record = style_record
      .iter_mut()
      .map(|(selector, style_value)| {
        (
          selector.to_owned(),
          style_value
            .iter_mut()
            .reduce(|a, b| {
              for (key, value) in b.iter() {
                let has_property_index = a.iter().position(|property| property.0 == key.to_owned());
                if let Some(index) = has_property_index {
                  a[index] = (key.to_owned(), value.clone());
                } else {
                  a.push((key.to_owned(), value.clone()));
                }
              }
              a
            })
            .unwrap()
            .to_owned()
        )
      })
      .collect::<HashMap<_, _>>();

          // 进行样式解析优化，提前解析 ArkUI 的样式，减少运行时的计算
    let final_all_style = final_all_style
    .iter_mut()
    .map(|(selector, properties)| {
      (
        selector.to_owned(),
        parse_style_properties(
          &properties
            .iter()
            .map(|(k, v)| (k.to_owned(), v.clone()))
            .collect::<Vec<_>>(),
        ),
      )
    })
    .collect::<HashMap<_, _>>();

    final_all_style
  }

  // 合并相同类型的 style，比如 .a { color: red } .a { color: blue } => .a { color: blue }，并且 !important 的优先级高于普通的
  fn calc_style_record<T: Hash + Eq + Clone>(
    &self,
    style_record: &mut Vec<(T, Vec<StyleDeclaration<'i>>)>,
  ) -> Vec<(T, StyleDeclaration<'i>)> {
    // 创建一个新的向量 final_style_record，用于存储最终的样式记录
    let mut final_style_record = vec![];
    // 对输入的 style_record 中的每个元素进行迭代
    for (id, declarations) in style_record.iter_mut() {
       // 对每个 declarations 中的 StyleDeclaration 进行按 specificity 排序
      declarations.sort_by(|a, b| a.specificity.cmp(&b.specificity));
      let mut final_properties: Vec<Property<'i>> = Vec::new();
      for declaration in declarations.iter() {
        let declaration = &declaration.declaration;
        let declarations = &declaration.declarations;
        for declaration in declarations.iter() {
          let has_property_index = final_properties
            .iter()
            .position(|property| property.property_id() == declaration.property_id());
          if let Some(index) = has_property_index {
            final_properties[index] = declaration.clone();
          } else {
            final_properties.push(declaration.clone());
          }
        }
      }
      for declaration in declarations.iter() {
        let declaration = &declaration.declaration;
        let important_declarations = &declaration.important_declarations;
        for declaration in important_declarations.iter() {
          let has_property_index = final_properties
            .iter()
            .position(|property| property.property_id() == declaration.property_id());
          if let Some(index) = has_property_index {
            final_properties[index] = declaration.clone();
          } else {
            final_properties.push(declaration.clone());
          }
        }
      }
      final_style_record.push((
        (*id).clone(),
        StyleDeclaration {
          specificity: 0,
          declaration: DeclarationBlock {
            declarations: final_properties,
            important_declarations: vec![],
          },
        },
      ));
    }
    final_style_record
  }
}
