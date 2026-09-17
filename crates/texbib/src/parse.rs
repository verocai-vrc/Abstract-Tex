//! The parser: a `.bib` file in, a [`Bibliography`] of [`Item`]s out, never an `Err`.
//!
//! Owns: the grammar BibTeX actually accepts (which is looser than any document of it), byte
//! spans for everything, and recovery — a malformed item costs that item and nothing else. It
//! must never read a file or know about the editor (see `lib.rs`).
//!
//! **What the grammar is.** Outside an `@`, BibTeX ignores everything, so free text between
//! entries is legal and common (`% Encoding: UTF-8`, a note from a supervisor). An item is
//! `@type` followed by a body in `{...}` or `(...)`. `@comment` is skipped, `@preamble` holds
//! one value, `@string` holds `name = value`, and anything else is an entry: a key, then
//! `name = value` fields separated by commas, a trailing comma allowed. Names — of entry types,
//! fields, keys and macros — are any run of characters that is not whitespace and not one of
//! `" # % ' ( ) , = { }`; that is BibTeX's own definition and it is why `smith:2019` and
//! `van-der-berg_2020` are valid keys. This parser also excludes `@`, which BibTeX technically
//! allows in a name: no real file uses one, and treating it as "the next item starts here"
//! turns a missing value at the end of an entry into a one-line error instead of swallowing
//! the following entry as a macro name. Values are described in `value.rs`.
//!
//! **What recovery is.** On an error the item becomes [`Item::Error`] and parsing resumes at
//! the next `@` that starts a line (after optional indentation), or failing that the next `@`
//! anywhere, or the end of the file. Starting a line is the tie-breaker because an `@` inside
//! the broken entry's own `email = {a@b.org}` would otherwise be mistaken for the next item.
//!
//! **What a span is.** Byte offsets into the text given to [`parse()`], `start` inclusive and
//! `end` exclusive, the same convention as `str` slicing. The items' spans are contiguous and
//! cover the whole input — free text between items is an item too — so concatenating every
//! item's text reproduces the file byte for byte, which is the property a later loop's "append
//! one entry and touch nothing else" edit rests on. The fixture harness checks it for every
//! fixture.

use crate::value::{builtin_month, Value, ValuePart};

/// A byte range in the parsed text, `start` inclusive, `end` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Span {
    /// Byte offset of the first character.
    pub start: usize,
    /// Byte offset one past the last character.
    pub end: usize,
}

impl Span {
    /// The text this span covers, out of the same string that was parsed.
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

/// A whole `.bib` file, as the sequence of items it is written as.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Bibliography {
    /// In file order, spans contiguous from `0` to the end of the input.
    pub items: Vec<Item>,
}

/// One thing in a `.bib` file. Every variant carries its own span, and the variants are all
/// that can appear: the parser never drops text, so the file is exactly this list.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "item", rename_all = "camelCase")]
pub enum Item {
    /// `@article{key, ...}` and every other `@type` that is not one of the three below.
    Entry(Entry),
    /// `@string{name = value}` — a macro other values refer to by name.
    String(StringDef),
    /// `@preamble{value}` — TeX that BibTeX copies into the `.bbl` before the entries.
    Preamble(Preamble),
    /// `@comment{...}`, or free text between items, which BibTeX ignores just the same.
    Comment(Comment),
    /// An item that could not be parsed. The span runs from its `@` to wherever parsing
    /// resumed, so a caller can show exactly what was skipped.
    Error(ParseError),
}

impl Item {
    /// Where this item sits in the text.
    pub fn span(&self) -> Span {
        match self {
            Item::Entry(entry) => entry.span,
            Item::String(string) => string.span,
            Item::Preamble(preamble) => preamble.span,
            Item::Comment(comment) => comment.span,
            Item::Error(error) => error.span,
        }
    }
}

/// `@article{smith2019, author = {...}, ...}`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// From the `@` to the closing `}` or `)`.
    pub span: Span,
    /// The entry type as written (`article`, `Article`, `ARTICLE` are all seen in the wild).
    /// Compare with [`Entry::is_type`], not `==`.
    pub entry_type: String,
    /// The citation key, exactly as written. BibTeX compares keys case-insensitively and
    /// Biber case-sensitively; this crate stores what it saw and leaves the comparison to
    /// whoever knows which engine is in use.
    pub key: String,
    /// Where the key sits, for a rename-in-place.
    pub key_span: Span,
    /// In the order written. Duplicated field names are kept as duplicates: BibTeX takes the
    /// first, Biber the last, and a health check wants to see both.
    pub fields: Vec<Field>,
}

impl Entry {
    /// Whether this entry is of `entry_type`, ignoring case: `entry.is_type("article")`.
    pub fn is_type(&self, entry_type: &str) -> bool {
        self.entry_type.eq_ignore_ascii_case(entry_type)
    }

    /// The first field named `name`, ignoring case.
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|field| field.name.eq_ignore_ascii_case(name))
    }
}

/// `author = {Smith, Jane and Doe, John}` inside an entry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    /// From the first character of the name to the end of the value — not including the
    /// comma after it, which belongs to the entry.
    pub span: Span,
    /// As written; compare ignoring case.
    pub name: String,
    /// Where the value sits, so it can be replaced on its own.
    pub value_span: Span,
    /// The value, as written.
    pub value: Value,
}

/// `@string{ieee = "IEEE"}`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StringDef {
    /// From the `@` to the closing delimiter.
    pub span: Span,
    /// As written; macro references compare ignoring case.
    pub name: String,
    /// Where the value sits.
    pub value_span: Span,
    /// The value, which may itself refer to earlier macros.
    pub value: Value,
}

/// `@preamble{"\newcommand{\noop}[1]{}"}`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preamble {
    /// From the `@` to the closing delimiter.
    pub span: Span,
    /// The value, as written.
    pub value: Value,
}

/// Text BibTeX ignores: an `@comment{...}` item, or whatever sits between two items.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    /// The whole stretch, including the `@comment` and its braces when it is one.
    pub span: Span,
    /// The text: the braces' contents for an `@comment{...}`, otherwise the stretch itself.
    pub text: String,
}

/// An item the parser could not make sense of.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseError {
    /// From the item's `@` to where parsing resumed — everything that was skipped.
    pub span: Span,
    /// Byte offset of the character the parser was looking at when it gave up.
    pub at: usize,
    /// One sentence, for a person: `expected '=' after field name 'author'`.
    pub message: String,
}

impl Bibliography {
    /// Every entry, in file order, skipping the other item kinds.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.items.iter().filter_map(|item| match item {
            Item::Entry(entry) => Some(entry),
            _ => None,
        })
    }

    /// Every `@string` definition, in file order.
    pub fn strings(&self) -> impl Iterator<Item = &StringDef> {
        self.items.iter().filter_map(|item| match item {
            Item::String(string) => Some(string),
            _ => None,
        })
    }

    /// Every item that failed to parse, in file order.
    pub fn errors(&self) -> impl Iterator<Item = &ParseError> {
        self.items.iter().filter_map(|item| match item {
            Item::Error(error) => Some(error),
            _ => None,
        })
    }

    /// The first entry whose key is exactly `key`.
    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries().find(|entry| entry.key == key)
    }

    /// The text a macro stands for in this file: the last `@string` of that name (ignoring
    /// case), resolved in turn, or one of BibTeX's built-in months. `None` if neither.
    ///
    /// "Last" rather than BibTeX's "must be defined before use" because the lookup is by name
    /// across the whole file: a file that defines `ieee` twice is a health-check finding, not
    /// something this method should silently pick a side on by position.
    pub fn string(&self, name: &str) -> Option<String> {
        let own = self.strings().filter(|string| string.name.eq_ignore_ascii_case(name)).last();
        match own {
            Some(definition) => Some(self.resolve(&definition.value)),
            None => builtin_month(name).map(str::to_string),
        }
    }

    /// `value` as text, with this file's own `@string`s and the built-in months substituted.
    /// See [`Value::resolve_with`] for what happens to whitespace, braces and unknown macros.
    pub fn resolve(&self, value: &Value) -> String {
        value.resolve_with(|name| self.string(name))
    }
}

/// Parse a whole `.bib` file. Never fails; see the module doc for what happens instead.
pub fn parse(source: &str) -> Bibliography {
    let mut items = Vec::new();
    let mut position = 0;

    while position < source.len() {
        // Everything up to the next `@` is free text BibTeX ignores.
        let Some(at) = source[position..].find('@').map(|offset| position + offset) else {
            items.push(free_text(source, position, source.len()));
            break;
        };
        if at > position {
            items.push(free_text(source, position, at));
        }

        let mut cursor = Cursor { source, position: at + 1 };
        match parse_item(&mut cursor, at) {
            Ok(item) => {
                position = cursor.position;
                items.push(item);
            }
            Err(error) => {
                let resume = recovery_point(source, at + 1);
                items.push(Item::Error(ParseError {
                    span: Span { start: at, end: resume },
                    at: error.at,
                    message: error.message,
                }));
                position = resume;
            }
        }
    }

    Bibliography { items }
}

fn free_text(source: &str, start: usize, end: usize) -> Item {
    Item::Comment(Comment { span: Span { start, end }, text: source[start..end].to_string() })
}

/// Where to resume after a broken item that began before `from`: the next `@` that starts a
/// line, else the next `@` at all, else the end of the input.
fn recovery_point(source: &str, from: usize) -> usize {
    let mut search = from;
    let mut first_any: Option<usize> = None;
    while let Some(offset) = source[search..].find('@') {
        let at = search + offset;
        first_any.get_or_insert(at);
        let line_start = source[..at].rfind('\n').map_or(0, |newline| newline + 1);
        if source[line_start..at].trim().is_empty() && at > from {
            return at;
        }
        search = at + 1;
    }
    first_any.unwrap_or(source.len())
}

/// Why an item could not be parsed, before it is wrapped into a [`ParseError`] with its span.
struct Failure {
    at: usize,
    message: String,
}

/// A reading position in the source. Every `parse_*` function below advances it; on failure
/// the position is wherever the problem was seen, which is what `ParseError::at` reports.
struct Cursor<'a> {
    source: &'a str,
    position: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<char> {
        self.source[self.position..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.position += c.len_utf8();
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn expect(&mut self, wanted: char, context: &str) -> Result<(), Failure> {
        if self.peek() == Some(wanted) {
            self.bump();
            Ok(())
        } else {
            Err(self.fail(format!("expected '{wanted}' {context}")))
        }
    }

    fn fail(&self, message: String) -> Failure {
        let message = match self.peek() {
            Some(c) => format!("{message}, found '{c}'"),
            None => format!("{message}, reached the end of the file"),
        };
        Failure { at: self.position, message }
    }

    /// A name in BibTeX's sense: see the module doc for the characters it excludes.
    fn take_name(&mut self) -> &'a str {
        let start = self.position;
        while self.peek().is_some_and(is_name_char) {
            self.bump();
        }
        &self.source[start..self.position]
    }

    fn take_digits(&mut self) -> &'a str {
        let start = self.position;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        &self.source[start..self.position]
    }

    /// Consume text up to the `}` that balances an already-consumed `{`, returning the inside.
    fn take_balanced(&mut self) -> Result<&'a str, Failure> {
        let start = self.position;
        let mut depth = 1;
        while let Some(c) = self.peek() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let inside = &self.source[start..self.position];
                        self.bump();
                        return Ok(inside);
                    }
                }
                _ => {}
            }
            self.bump();
        }
        Err(Failure { at: start - 1, message: "a '{' is never closed".to_string() })
    }

    /// Consume text up to the `"` that ends an already-consumed `"`, returning the inside. A
    /// `"` inside braces does not count, per BibTeX.
    fn take_quoted(&mut self) -> Result<&'a str, Failure> {
        let start = self.position;
        let mut depth = 0;
        while let Some(c) = self.peek() {
            match c {
                '{' => depth += 1,
                '}' if depth > 0 => depth -= 1,
                '"' if depth == 0 => {
                    let inside = &self.source[start..self.position];
                    self.bump();
                    return Ok(inside);
                }
                _ => {}
            }
            self.bump();
        }
        Err(Failure { at: start - 1, message: "a '\"' is never closed".to_string() })
    }
}

fn is_name_char(c: char) -> bool {
    !c.is_whitespace() && !matches!(c, '"' | '#' | '%' | '\'' | '(' | ')' | ',' | '=' | '{' | '}' | '@')
}

/// Parse one item whose `@` is at `at`; the cursor sits just after that `@`.
fn parse_item(cursor: &mut Cursor, at: usize) -> Result<Item, Failure> {
    let kind = cursor.take_name().to_string();
    if kind.is_empty() {
        return Err(cursor.fail("expected an entry type after '@'".to_string()));
    }
    cursor.skip_whitespace();

    if kind.eq_ignore_ascii_case("comment") {
        return parse_comment(cursor, at);
    }

    // Either delimiter is legal, and the closer must match the opener.
    let closer = match cursor.peek() {
        Some('{') => '}',
        Some('(') => ')',
        _ => return Err(cursor.fail(format!("expected '{{' or '(' after '@{kind}'"))),
    };
    cursor.bump();
    cursor.skip_whitespace();

    if kind.eq_ignore_ascii_case("preamble") {
        let (value, _value_span) = parse_value(cursor)?;
        cursor.skip_whitespace();
        cursor.expect(closer, "to close @preamble")?;
        return Ok(Item::Preamble(Preamble { span: Span { start: at, end: cursor.position }, value }));
    }

    if kind.eq_ignore_ascii_case("string") {
        let name = cursor.take_name().to_string();
        if name.is_empty() {
            return Err(cursor.fail("expected a macro name after '@string{'".to_string()));
        }
        cursor.skip_whitespace();
        cursor.expect('=', &format!("after macro name '{name}'"))?;
        cursor.skip_whitespace();
        let (value, value_span) = parse_value(cursor)?;
        cursor.skip_whitespace();
        if cursor.peek() == Some(',') {
            cursor.bump();
            cursor.skip_whitespace();
        }
        cursor.expect(closer, "to close @string")?;
        return Ok(Item::String(StringDef { span: Span { start: at, end: cursor.position }, name, value_span, value }));
    }

    parse_entry(cursor, at, kind, closer)
}

/// `@comment{...}` with balanced braces, as Biber reads it; without a brace, the rest of the
/// line, which is the closest thing to what BibTeX does (it ignores the text either way).
fn parse_comment(cursor: &mut Cursor, at: usize) -> Result<Item, Failure> {
    let text = if cursor.peek() == Some('{') {
        cursor.bump();
        cursor.take_balanced()?.to_string()
    } else {
        let start = cursor.position;
        while cursor.peek().is_some_and(|c| c != '\n') {
            cursor.bump();
        }
        cursor.source[start..cursor.position].to_string()
    };
    Ok(Item::Comment(Comment { span: Span { start: at, end: cursor.position }, text }))
}

fn parse_entry(cursor: &mut Cursor, at: usize, entry_type: String, closer: char) -> Result<Item, Failure> {
    let key_start = cursor.position;
    let key = cursor.take_name().to_string();
    if key.is_empty() {
        return Err(cursor.fail(format!("expected a citation key after '@{entry_type}{{'")));
    }
    let key_span = Span { start: key_start, end: cursor.position };
    cursor.skip_whitespace();

    let mut fields = Vec::new();
    loop {
        if cursor.peek() == Some(closer) {
            cursor.bump();
            break;
        }
        cursor.expect(',', &format!("between fields of '{key}'"))?;
        cursor.skip_whitespace();
        // A trailing comma before the closer is legal and very common.
        if cursor.peek() == Some(closer) {
            cursor.bump();
            break;
        }

        let field_start = cursor.position;
        let name = cursor.take_name().to_string();
        if name.is_empty() {
            return Err(cursor.fail(format!("expected a field name in '{key}'")));
        }
        cursor.skip_whitespace();
        cursor.expect('=', &format!("after field name '{name}'"))?;
        cursor.skip_whitespace();
        let (value, value_span) = parse_value(cursor)?;
        fields.push(Field { span: Span { start: field_start, end: cursor.position }, name, value_span, value });
        cursor.skip_whitespace();
    }

    Ok(Item::Entry(Entry { span: Span { start: at, end: cursor.position }, entry_type, key, key_span, fields }))
}

/// One value: parts joined by `#`. The cursor sits on the first character of the value.
fn parse_value(cursor: &mut Cursor) -> Result<(Value, Span), Failure> {
    let start = cursor.position;
    let mut parts = Vec::new();
    loop {
        let part = match cursor.peek() {
            Some('{') => {
                cursor.bump();
                ValuePart::Braced(cursor.take_balanced()?.to_string())
            }
            Some('"') => {
                cursor.bump();
                ValuePart::Quoted(cursor.take_quoted()?.to_string())
            }
            Some(c) if c.is_ascii_digit() => ValuePart::Number(cursor.take_digits().to_string()),
            Some(c) if is_name_char(c) => ValuePart::Macro(cursor.take_name().to_string()),
            _ => return Err(cursor.fail("expected a value".to_string())),
        };
        parts.push(part);
        let end = cursor.position;

        // Only a `#` continues the value; anything else belongs to whoever called us.
        cursor.skip_whitespace();
        if cursor.peek() == Some('#') {
            cursor.bump();
            cursor.skip_whitespace();
            continue;
        }
        cursor.position = end;
        return Ok((Value { parts }, Span { start, end }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only_entry(source: &str) -> Entry {
        let bib = parse(source);
        let entries: Vec<_> = bib.entries().cloned().collect();
        assert_eq!(entries.len(), 1, "{bib:#?}");
        entries.into_iter().next().unwrap()
    }

    fn spans_cover_the_input(source: &str) {
        let bib = parse(source);
        let mut rebuilt = String::new();
        let mut expected_start = 0;
        for item in &bib.items {
            let span = item.span();
            assert_eq!(span.start, expected_start, "gap or overlap before {item:?}");
            rebuilt.push_str(span.text(source));
            expected_start = span.end;
        }
        assert_eq!(rebuilt, source);
    }

    #[test]
    fn an_empty_file_has_no_items() {
        assert!(parse("").items.is_empty());
    }

    #[test]
    fn whitespace_alone_is_one_comment() {
        let bib = parse("  \n\n");
        assert_eq!(bib.items.len(), 1);
        assert!(matches!(&bib.items[0], Item::Comment(c) if c.text == "  \n\n"));
    }

    #[test]
    fn a_plain_entry_with_braced_and_quoted_values() {
        let entry = only_entry("@article{smith2019,\n  author = {Smith, Jane},\n  title = \"A Title\",\n  year = 2019\n}");
        assert!(entry.is_type("Article"));
        assert_eq!(entry.key, "smith2019");
        assert_eq!(entry.fields.len(), 3);
        assert_eq!(entry.field("AUTHOR").unwrap().value.parts, vec![ValuePart::Braced("Smith, Jane".into())]);
        assert_eq!(entry.field("title").unwrap().value.parts, vec![ValuePart::Quoted("A Title".into())]);
        assert_eq!(entry.field("year").unwrap().value.parts, vec![ValuePart::Number("2019".into())]);
    }

    #[test]
    fn parentheses_delimit_an_entry_too() {
        let entry = only_entry("@book(knuth84, title = {The TeXbook})");
        assert_eq!(entry.key, "knuth84");
        assert_eq!(entry.span.text("@book(knuth84, title = {The TeXbook})"), "@book(knuth84, title = {The TeXbook})");
    }

    #[test]
    fn a_trailing_comma_is_fine() {
        let entry = only_entry("@misc{k, title = {T},\n}");
        assert_eq!(entry.fields.len(), 1);
    }

    #[test]
    fn a_key_may_contain_colons_dashes_and_dots() {
        let entry = only_entry("@misc{van-der-berg:2020.v2, title = {T}}");
        assert_eq!(entry.key, "van-der-berg:2020.v2");
    }

    #[test]
    fn nested_braces_and_a_quote_inside_braces_survive() {
        let entry = only_entry(r#"@misc{k, title = {The {DNA} of "quotes"}, note = "a {"} brace"}"#);
        assert_eq!(entry.field("title").unwrap().value.parts, vec![ValuePart::Braced(r#"The {DNA} of "quotes""#.into())]);
        assert_eq!(entry.field("note").unwrap().value.parts, vec![ValuePart::Quoted(r#"a {"} brace"#.into())]);
    }

    #[test]
    fn concatenation_with_hash_keeps_every_part() {
        let source = "@string{ieee = \"IEEE\"}\n@article{k, journal = ieee # \" Trans. \" # 12}";
        let bib = parse(source);
        let entry = bib.entries().next().unwrap();
        let journal = entry.field("journal").unwrap();
        assert_eq!(journal.value.parts.len(), 3);
        assert_eq!(journal.value_span.text(source), "ieee # \" Trans. \" # 12");
        assert_eq!(bib.resolve(&journal.value), "IEEE Trans. 12");
    }

    #[test]
    fn a_month_macro_resolves_without_a_string_definition() {
        let bib = parse("@article{k, month = sep}");
        let month = &bib.entries().next().unwrap().field("month").unwrap().value;
        assert_eq!(bib.resolve(month), "September");
        assert!(bib.string("nosuch").is_none());
    }

    #[test]
    fn a_file_may_redefine_a_built_in_month() {
        let bib = parse("@string{jan = \"Jan.\"}\n@article{k, month = jan}");
        assert_eq!(bib.string("jan").as_deref(), Some("Jan."));
        assert_eq!(bib.string("feb").as_deref(), Some("February"));
    }

    #[test]
    fn strings_may_refer_to_earlier_strings() {
        let bib = parse("@string{pub = \"ACM\"}\n@string{conf = pub # \" SIGPLAN\"}");
        assert_eq!(bib.string("CONF").as_deref(), Some("ACM SIGPLAN"));
    }

    #[test]
    fn preamble_is_its_own_item() {
        let bib = parse("@preamble{\"\\newcommand{\\noop}[1]{}\"}");
        assert!(matches!(&bib.items[0], Item::Preamble(p) if p.value.parts.len() == 1));
    }

    #[test]
    fn comment_items_and_free_text_are_both_comments() {
        let source = "% header\n@comment{ignored {nested} }\n@Comment jabref rest of line\n@misc{k, title = {T}}\n";
        let bib = parse(source);
        let kinds: Vec<&str> = bib
            .items
            .iter()
            .map(|item| match item {
                Item::Comment(_) => "comment",
                Item::Entry(_) => "entry",
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(kinds, ["comment", "comment", "comment", "comment", "comment", "entry", "comment"]);
        assert!(matches!(&bib.items[1], Item::Comment(c) if c.text == "ignored {nested} "));
        assert!(matches!(&bib.items[3], Item::Comment(c) if c.text == "jabref rest of line"));
        spans_cover_the_input(source);
    }

    #[test]
    fn a_broken_entry_costs_only_itself() {
        let source = "@misc{a, title = {A}}\n@misc{b, title = \n@misc{c, title = {C}}\n";
        let bib = parse(source);
        let keys: Vec<&str> = bib.entries().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, ["a", "c"]);
        let error = bib.errors().next().expect("the middle entry is an error");
        assert_eq!(error.span.text(source), "@misc{b, title = \n");
        assert!(error.message.starts_with("expected a value"), "{}", error.message);
        spans_cover_the_input(source);
    }

    #[test]
    fn recovery_prefers_an_at_that_starts_a_line() {
        // The `@` in the email must not be taken for the next item.
        let source = "@misc{b, email = {a@b.org}, title = \n@misc{c, title = {C}}";
        let bib = parse(source);
        assert_eq!(bib.entries().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["c"]);
        spans_cover_the_input(source);
    }

    #[test]
    fn an_unclosed_brace_is_reported_at_the_brace() {
        let source = "@misc{k, title = {never closed";
        let bib = parse(source);
        let error = bib.errors().next().unwrap();
        assert_eq!(error.at, source.find("{never").unwrap());
        assert_eq!(error.message, "a '{' is never closed");
        assert_eq!(error.span.end, source.len());
    }

    #[test]
    fn a_stray_at_in_free_text_is_an_error_not_a_crash() {
        let source = "contact me @ the office\n@misc{k, title = {T}}";
        let bib = parse(source);
        assert_eq!(bib.errors().count(), 1);
        assert_eq!(bib.entries().count(), 1);
        spans_cover_the_input(source);
    }

    #[test]
    fn crlf_and_unicode_do_not_disturb_spans() {
        let source = "@misc{k,\r\n  author = {Ærø, Søren},\r\n  title = {Ünïcödé}\r\n}\r\n";
        let entry = only_entry(source);
        assert_eq!(entry.field("author").unwrap().value_span.text(source), "{Ærø, Søren}");
        spans_cover_the_input(source);
    }

    #[test]
    fn field_spans_exclude_the_separating_comma() {
        let source = "@misc{k, title = {T}  ,\n  year = 2019 }";
        let entry = only_entry(source);
        assert_eq!(entry.fields[0].span.text(source), "title = {T}");
        assert_eq!(entry.fields[1].span.text(source), "year = 2019");
        assert_eq!(entry.key_span.text(source), "k");
    }

    #[test]
    fn duplicate_fields_are_kept_and_field_returns_the_first() {
        let entry = only_entry("@misc{k, year = 1, year = 2}");
        assert_eq!(entry.fields.len(), 2);
        assert_eq!(entry.field("year").unwrap().value.parts, vec![ValuePart::Number("1".into())]);
    }

    #[test]
    fn errors_name_the_key_and_the_field() {
        let bib = parse("@misc{k, title {T}}");
        assert_eq!(bib.errors().next().unwrap().message, "expected '=' after field name 'title', found '{'");
        let bib = parse("@misc{, title = {T}}");
        assert!(bib.errors().next().unwrap().message.starts_with("expected a citation key after '@misc{'"));
        let bib = parse("@misc k");
        assert!(bib.errors().next().unwrap().message.starts_with("expected '{' or '(' after '@misc'"));
    }
}
