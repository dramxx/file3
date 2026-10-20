//! Go to symbol: functions, types and headings found by the syntax grammars
//! (the `entity.name.*` scopes that highlighting already computes).

use syntect::parsing::{ParseState, Scope, ScopeStack};

use crate::finder::Finder;
use crate::syntax::{find_syntax, get_syntax_set};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Function,
    Type,
    Module,
    Heading,
}

impl Kind {
    /// One-letter badge shown in the picker.
    pub fn badge(self) -> &'static str {
        match self {
            Kind::Function => "ƒ",
            Kind::Type => "T",
            Kind::Module => "M",
            Kind::Heading => "#",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: Kind,
    /// 0-based line.
    pub line: usize,
}

/// The Ctrl+R / Ctrl+Shift+O picker.
pub struct SymbolPicker {
    pub picker: Finder,
    pub symbols: Vec<Symbol>,
    labels: Vec<String>,
    /// Where the file was scrolled before, restored on Esc.
    pub origin_scroll: usize,
}

impl SymbolPicker {
    /// Starts on the symbol at or above `current_line` (0-based).
    pub fn new(symbols: Vec<Symbol>, current_line: usize, origin_scroll: usize) -> Self {
        let labels: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();
        let mut picker = Finder::new(&labels);
        picker.selected = symbols
            .iter()
            .rposition(|s| s.line <= current_line)
            .unwrap_or(0);
        Self {
            picker,
            symbols,
            labels,
            origin_scroll,
        }
    }

    pub fn update(&mut self) {
        self.picker.update(&self.labels);
    }

    pub fn selected(&self) -> Option<&Symbol> {
        self.picker
            .results
            .get(self.picker.selected)
            .map(|r| &self.symbols[r.index])
    }
}

/// Same limit as highlighting: very long lines are skipped.
const MAX_LINE_WIDTH: usize = 2000;

pub fn find_symbols(lines: &[String], file_name: &str) -> Vec<Symbol> {
    let syntax_set = get_syntax_set();
    let syntax = find_syntax(syntax_set, file_name);
    let kinds: Vec<(Scope, Kind)> = [
        ("entity.name.function", Kind::Function),
        ("entity.name.macro", Kind::Function),
        ("entity.name.class", Kind::Type),
        ("entity.name.struct", Kind::Type),
        ("entity.name.enum", Kind::Type),
        ("entity.name.union", Kind::Type),
        ("entity.name.trait", Kind::Type),
        ("entity.name.interface", Kind::Type),
        ("entity.name.impl", Kind::Type),
        ("entity.name.type", Kind::Type),
        ("entity.name.namespace", Kind::Module),
        ("entity.name.module", Kind::Module),
        ("entity.name.section", Kind::Heading),
    ]
    .iter()
    .filter_map(|(s, k)| Scope::new(s).ok().map(|s| (s, *k)))
    .collect();
    // Names in these contexts are uses (calls, type annotations), not definitions.
    let uses: Vec<Scope> = [
        "meta.function-call",
        "meta.type.annotation",
        "meta.return.type",
        "meta.type.parameters",
        "meta.function.parameters",
        "meta.function.return-type",
    ]
    .iter()
    .filter_map(|s| Scope::new(s).ok())
    .collect();

    let kind_of = |stack: &ScopeStack| -> Option<Kind> {
        let scopes = stack.as_slice();
        if scopes.iter().any(|s| uses.iter().any(|u| u.is_prefix_of(*s))) {
            return None;
        }
        scopes.iter().rev().find_map(|s| {
            kinds
                .iter()
                .find(|(prefix, _)| prefix.is_prefix_of(*s))
                .map(|(_, k)| *k)
        })
    };

    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut symbols = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.len() > MAX_LINE_WIDTH {
            continue;
        }
        let text = format!("{}\n", line);
        let Ok(ops) = state.parse_line(&text, syntax_set) else { break };

        // Text pieces between scope changes; consecutive pieces of one name are joined.
        let mut current: Option<(Kind, String)> = None;
        let mut last = 0;
        let mut take_piece = |stack: &ScopeStack, piece: &str, current: &mut Option<(Kind, String)>| {
            match (kind_of(stack), current.as_mut()) {
                (Some(kind), Some((k, name))) if *k == kind => name.push_str(piece),
                (Some(kind), _) => {
                    push_symbol(&mut symbols, current.take(), i);
                    *current = Some((kind, piece.to_string()));
                }
                (None, _) => push_symbol(&mut symbols, current.take(), i),
            }
        };
        for (pos, op) in ops {
            if pos > last {
                take_piece(&stack, &text[last..pos], &mut current);
                last = pos;
            }
            let _ = stack.apply(&op);
        }
        take_piece(&stack, &text[last..], &mut current);
        push_symbol(&mut symbols, current.take(), i);
    }
    symbols
}

fn push_symbol(symbols: &mut Vec<Symbol>, found: Option<(Kind, String)>, line: usize) {
    if let Some((kind, name)) = found {
        let name = name.trim();
        if !name.is_empty() && symbols.last().is_none_or(|s| s.line != line || s.name != name) {
            symbols.push(Symbol {
                name: name.to_string(),
                kind,
                line,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(code: &str) -> Vec<String> {
        code.lines().map(str::to_string).collect()
    }

    fn names(symbols: &[Symbol]) -> Vec<(&str, Kind, usize)> {
        symbols.iter().map(|s| (s.name.as_str(), s.kind, s.line)).collect()
    }

    #[test]
    fn test_rust_symbols() {
        let code = "struct App {}\n\nimpl App {\n    fn new() -> Self { helper(); App {} }\n}\n\nenum Mode { A }\n";
        assert_eq!(
            names(&find_symbols(&lines(code), "main.rs")),
            vec![
                ("App", Kind::Type, 0),
                ("App", Kind::Type, 2),
                ("new", Kind::Function, 3),
                ("Mode", Kind::Type, 6),
            ]
        );
    }

    #[test]
    fn test_python_symbols() {
        let code = "class Shop:\n    def buy(self):\n        print('x')\n";
        assert_eq!(
            names(&find_symbols(&lines(code), "shop.py")),
            vec![("Shop", Kind::Type, 0), ("buy", Kind::Function, 1)]
        );
    }

    #[test]
    fn test_markdown_headings() {
        let code = "# Title\n\ntext\n\n## Part two\n";
        assert_eq!(
            names(&find_symbols(&lines(code), "README.md")),
            vec![("Title", Kind::Heading, 0), ("Part two", Kind::Heading, 4)]
        );
    }

    #[test]
    fn test_typescript_skips_type_annotations() {
        let code = "export class Store {\n  private items: Item[] = [];\n  add(item: Item): void { this.items.push(item); }\n}\nexport function total(x: number) { return x; }\nconst handler = async () => {};\ninterface Item { id: string }\ntype Id = string;\n";
        assert_eq!(
            names(&find_symbols(&lines(code), "store.ts")),
            vec![
                ("Store", Kind::Type, 0),
                ("add", Kind::Function, 2),
                ("total", Kind::Function, 4),
                ("handler", Kind::Function, 5),
                ("Item", Kind::Type, 6),
                ("Id", Kind::Type, 7),
            ]
        );
    }

    #[test]
    fn test_csharp_and_rust_parameters() {
        let cs = "namespace Shop {\n  public class Cart {\n    public void Add(int x) { Console.WriteLine(x); }\n  }\n}\n";
        assert_eq!(
            names(&find_symbols(&lines(cs), "cart.cs")),
            vec![("Shop", Kind::Module, 0), ("Cart", Kind::Type, 1), ("Add", Kind::Function, 2)]
        );
        let rs = "pub fn run(app: &mut App, x: Vec<Item>) -> Result<Item> { Ok(x) }\nimpl Display for App {}\n";
        assert_eq!(
            names(&find_symbols(&lines(rs), "run.rs")),
            vec![("run", Kind::Function, 0), ("App", Kind::Type, 1)]
        );
    }

    #[test]
    fn test_plain_text_has_none() {
        assert!(find_symbols(&lines("just words\n"), "notes.txt").is_empty());
    }
}
