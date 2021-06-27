use fluent_syntax::ast;
use fluent_syntax::parser;
use serde::Deserialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;

#[derive(Debug, Deserialize)]
struct Bundle {
    main: String,
    langs: HashMap<String, String>,
}

pub fn run(project: &str, token: &str, out_file: &str) {
    let bundle = fetch_bundle(&project, &token);

    let generated = generate_types(
        bundle
            .langs
            .get(&bundle.main)
            .expect("Main FTL does not exist!"),
    );

    let code = r#"
import { FluentBundle, FluentResource } from '@fluent/bundle'

export interface Lang {
    <K extends keyof LocalizedMessage>(
    id: K,
    ...params: LocalizedMessage[K]
    ): string
}

export function createLang(locale: keyof typeof langs): Lang {
    const bundle = new FluentBundle(locale)
    const resource = new FluentResource(langs[locale])
    bundle.addResource(resource)
    return (id, ...[params]) => {
    const message = bundle.getMessage(id)
    if (!message || !message.value) {
        return id
    }
    return bundle.formatPattern(message.value, params)
    }
}
    "#
    .trim()
    .to_string();

    let ftls = format!(
        "export const langs = {{\n  {}\n}} as const",
        bundle
            .langs
            .iter()
            .map(|(locale, ftl)| format!("'{}': `{}`", locale, ftl))
            .collect::<Vec<_>>()
            .join(",\n  ")
    );

    let output = vec![code, generated, ftls].join("\n\n");

    fs::write(&out_file, &output).expect("Failed to write file");

    println!(
        "Generated translations for {} languages",
        bundle.langs.len()
    );
}

fn fetch_bundle(project: &str, token: &str) -> Bundle {
    let client = reqwest::blocking::Client::new();

    let mut params = HashMap::new();
    params.insert("projectId", project);
    params.insert("token", token);

    let res = client
        .post("https://us-central1-turnslate.cloudfunctions.net/langs")
        .json(&params)
        .send()
        .expect("Failed to fetch data from the server");

    res.json::<Bundle>().expect("Failed to parse json")
}

fn generate_types(ftl: &str) -> String {
    let resource = parser::parse(ftl).expect("Failed to parse FTL");

    let ids = parse_nodes(&resource)
        .into_iter()
        .map(|node| {
            format!(
                "'{}': [{}],",
                node.name,
                if node.ids.len() > 0 {
                    format!(
                        "Vars<'{}'>",
                        node.ids.into_iter().collect::<Vec<_>>().join("' | '")
                    )
                } else {
                    String::from("")
                }
            )
        })
        .collect::<Vec<_>>();

    format!(
        r#"
export type LocalizedMessage = {{
  {}
}}

type Vars<T extends string> = Record<T, string | number>
        "#,
        ids.join("\n  ")
    )
}

#[derive(Debug, PartialEq, Eq)]
struct Node {
    name: String,
    comment: Option<String>,
    ids: HashSet<String>,
}

fn parse_nodes<'a>(ast: &'a ast::Resource<&str>) -> Vec<Node> {
    let mut result: Vec<Node> = Vec::new();

    for entry in &ast.body {
        match entry {
            ast::Entry::Message(msg) => result.push(visit_message(msg)),
            _ => {}
        }
    }

    result
}

fn visit_message(message: &ast::Message<&str>) -> Node {
    Node {
        name: message.id.name.to_string(),
        comment: None,
        ids: match &message.value {
            Some(value) => visit_pattern(value),
            None => HashSet::new(),
        },
    }
}

fn visit_pattern(pattern: &ast::Pattern<&str>) -> HashSet<String> {
    let mut result = HashSet::new();

    for element in &pattern.elements {
        match element {
            ast::PatternElement::Placeable { expression } => match expression {
                ast::Expression::Inline(expr) => match expr {
                    ast::InlineExpression::VariableReference { id } => {
                        result.insert(id.name.to_string());
                    }
                    _ => {}
                },
                ast::Expression::Select { selector, variants } => match selector {
                    ast::InlineExpression::VariableReference { id } => {
                        result.insert(id.name.to_string());
                        for variant in variants {
                            result.union(&visit_pattern(&variant.value));
                        }
                    }
                    _ => {}
                },
            },
            _ => {}
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_nodes_works() {
        let ftl = r#"
# Simple things are simple.
hello-user = Hello, {$userName}!"#;

        let resource = parser::parse(ftl).expect("Failed to parse FTL");
        let actual = parse_nodes(&resource);
        let expected = vec![Node {
            name: "hello-user".to_string(),
            comment: None,
            ids: vec!["userName"]
                .iter()
                .map(|x| x.to_string())
                .collect::<HashSet<_>>(),
        }];

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_nodes_works_with_complex_message() {
        let ftl = r#"
# Complex things are possible.
shared-photos =
    {$userName} {$photoCount ->
        [one] added a new photo
        *[other] added {$photoCount} new photos
    } to {$userGender ->
        [male] his stream
        [female] her stream
        *[other] their stream
    }."#;

        let resource = parser::parse(ftl).expect("Failed to parse FTL");
        let actual = parse_nodes(&resource);
        let expected = vec![Node {
            name: "shared-photos".to_string(),
            comment: None,
            ids: vec!["userName", "photoCount", "userGender"]
                .iter()
                .map(|x| x.to_string())
                .collect::<HashSet<_>>(),
        }];

        assert_eq!(expected, actual);
    }
}
