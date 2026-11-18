//! Coloration syntaxique légère : un seul passage sur le texte, sans dépendance.
//! En plus des mots-clés, chaînes et commentaires, on colore les types, les fonctions et les
//! constantes en MAJUSCULES, repérés en lisant le fichier (comme la version Python).
use crate::theme::Palette;
use eframe::egui::{text::LayoutJob, Color32, FontId, TextFormat};
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    C, Cpp, Java, CSharp, Js, Ts, Rust, Go, Kotlin, Python, Shell, PowerShell, Make, Json, Html, Css,
    Markdown, Sql, Lua, Php, Yaml, Toml, Plain,
}

impl Lang {
    pub fn detect(path: &str) -> Lang {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let lower = name.to_lowercase();
        if lower == "makefile" || lower == "gnumakefile" || lower.ends_with(".mk") {
            return Lang::Make;
        }
        if lower == "dockerfile" || lower == "cmakelists.txt" {
            return Lang::Shell;
        }
        match lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("") {
            "c" | "h" => Lang::C,
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" | "ino" => Lang::Cpp,
            "java" => Lang::Java,
            "cs" => Lang::CSharp,
            "js" | "mjs" | "cjs" | "jsx" => Lang::Js,
            "ts" | "tsx" => Lang::Ts,
            "rs" => Lang::Rust,
            "go" => Lang::Go,
            "kt" | "kts" | "gradle" | "groovy" => Lang::Kotlin,
            "py" | "pyw" | "pyi" => Lang::Python,
            "sh" | "bash" | "zsh" | "fish" | "env" => Lang::Shell,
            "ps1" | "psm1" | "bat" | "cmd" => Lang::PowerShell,
            "json" | "jsonc" => Lang::Json,
            "html" | "htm" | "xml" | "svg" | "xhtml" | "vue" => Lang::Html,
            "css" | "scss" | "less" => Lang::Css,
            "md" | "markdown" => Lang::Markdown,
            "sql" => Lang::Sql,
            "lua" => Lang::Lua,
            "php" => Lang::Php,
            "yml" | "yaml" => Lang::Yaml,
            "toml" | "ini" | "cfg" | "conf" | "properties" => Lang::Toml,
            _ => Lang::Plain,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::C => "C", Lang::Cpp => "C++", Lang::Java => "Java", Lang::CSharp => "C#",
            Lang::Js => "JavaScript", Lang::Ts => "TypeScript", Lang::Rust => "Rust", Lang::Go => "Go",
            Lang::Kotlin => "Kotlin", Lang::Python => "Python", Lang::Shell => "Shell",
            Lang::PowerShell => "PowerShell", Lang::Make => "Makefile", Lang::Json => "JSON",
            Lang::Html => "HTML", Lang::Css => "CSS", Lang::Markdown => "Markdown", Lang::Sql => "SQL",
            Lang::Lua => "Lua", Lang::Php => "PHP", Lang::Yaml => "YAML", Lang::Toml => "Config",
            Lang::Plain => "Texte",
        }
    }

    /// (début de commentaire de ligne, bloc ouvrant, bloc fermant)
    pub fn comments(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Lang::Python | Lang::Shell | Lang::Make | Lang::Yaml | Lang::Toml => ("#", "", ""),
            Lang::PowerShell => ("#", "<#", "#>"),
            Lang::Sql | Lang::Lua => ("--", "/*", "*/"),
            Lang::Html | Lang::Markdown => ("", "<!--", "-->"),
            Lang::Css => ("", "/*", "*/"),
            Lang::Json | Lang::Plain => ("", "", ""),
            Lang::Php => ("//", "/*", "*/"),
            _ => ("//", "/*", "*/"),
        }
    }

    fn brace_family(self) -> bool {
        matches!(self, Lang::C | Lang::Cpp | Lang::Java | Lang::CSharp | Lang::Js | Lang::Ts | Lang::Rust
            | Lang::Go | Lang::Kotlin | Lang::Php)
    }

    /// Les noms en CamelCase sont des classes (convention de ces langages).
    fn camel_types(self) -> bool {
        matches!(self, Lang::Java | Lang::CSharp | Lang::Js | Lang::Ts | Lang::Kotlin | Lang::Rust
            | Lang::Python | Lang::Php)
    }
}

const C_KW: &str = "auto break case char const continue default do double else enum extern float for goto if \
    inline int long register restrict return short signed sizeof static struct switch typedef union unsigned \
    void volatile while bool _Bool true false NULL";
const CPP_KW: &str = "alignas alignof and asm catch class constexpr const_cast decltype delete dynamic_cast \
    explicit export friend mutable namespace new noexcept not nullptr operator or private protected public \
    reinterpret_cast static_assert static_cast template this thread_local throw try typeid typename using \
    virtual override final concept requires co_await co_return co_yield";
const JAVA_KW: &str = "abstract assert boolean break byte case catch char class const continue default do double \
    else enum extends final finally float for goto if implements import instanceof int interface long native new \
    package private protected public return short static strictfp super switch synchronized this throw throws \
    transient try void volatile while var record sealed permits yield true false null";
const CS_KW: &str = "abstract as base bool break byte case catch char checked class const continue decimal default \
    delegate do double else enum event explicit extern false finally fixed float for foreach goto if implicit in \
    int interface internal is lock long namespace new null object operator out override params private protected \
    public readonly ref return sbyte sealed short sizeof stackalloc static string struct switch this throw true \
    try typeof uint ulong unchecked unsafe ushort using virtual void volatile while var async await get set \
    record init";
const JS_KW: &str = "break case catch class const continue debugger default delete do else export extends false \
    finally for function if import in instanceof let new null return super switch this throw true try typeof \
    undefined var void while with yield async await of static get set from as";
const TS_KW: &str = "interface type enum implements namespace declare abstract private protected public readonly \
    keyof infer is never unknown any number string boolean symbol bigint";
const RUST_KW: &str = "as async await break const continue crate dyn else enum extern false fn for if impl in let \
    loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while";
const RUST_TYPES: &str = "i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize f32 f64 bool char str String Vec \
    Option Result Box Rc Arc HashMap HashSet BTreeMap Some None Ok Err";
const GO_KW: &str = "break case chan const continue default defer else fallthrough for func go goto if import \
    interface map package range return select struct switch type var true false nil iota";
const GO_TYPES: &str = "bool byte complex64 complex128 error float32 float64 int int8 int16 int32 int64 rune \
    string uint uint8 uint16 uint32 uint64 uintptr any";
const KT_KW: &str = "as break class continue do else false for fun if in interface is null object package return \
    super this throw true try typealias val var when while by catch constructor companion data enum finally \
    get import init internal lateinit open override private protected public sealed set suspend def";
const PY_KW: &str = "False None True and as assert async await break class continue def del elif else except \
    finally for from global if import in is lambda nonlocal not or pass raise return try while with yield \
    match case self cls";
const PY_BUILTINS: &str = "print len range int str float list dict set tuple bool open input enumerate zip map \
    filter sorted reversed sum min max abs round isinstance type super repr hash iter next any all ord chr \
    format id dir vars getattr setattr hasattr divmod pow bytes bytearray object Exception ValueError \
    TypeError KeyError IndexError";
const SH_KW: &str = "if then else elif fi case esac for while until do done in function return local export \
    echo cd exit source read set unset shift break continue alias sudo";
const PS_KW: &str = "begin break catch class continue data do dynamicparam else elseif end exit filter finally \
    for foreach from function if in param process return switch throw trap try until using while";
const SQL_KW: &str = "select from where insert into values update set delete create table drop alter add primary \
    key foreign references join left right inner outer on group by order having limit offset as and or not \
    null is in like between distinct count sum avg min max union all exists case when then else end \
    int integer varchar text date default unique index";
const LUA_KW: &str = "and break do else elseif end false for function goto if in local nil not or repeat return \
    then true until while";
const PHP_KW: &str = "abstract and array as break case catch class clone const continue declare default do echo \
    else elseif empty enddeclare endfor endforeach endif endswitch endwhile extends final finally fn for foreach \
    function global if implements include instanceof interface isset list namespace new or print private \
    protected public require return static switch throw trait try unset use var while yield true false null";
const C_TYPES: &str = "size_t ssize_t ptrdiff_t intptr_t uintptr_t off_t time_t clock_t FILE va_list wchar_t \
    int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t string vector map set list pair \
    unique_ptr shared_ptr Vector2 Vector3 Vector4 Matrix Color Rectangle Image Texture Texture2D \
    RenderTexture2D Font Camera Camera2D Camera3D Mesh Model Shader Material Ray BoundingBox Sound Music Wave";

pub struct Rules {
    keywords: HashSet<&'static str>,
    types: HashSet<&'static str>,
    builtins: HashSet<&'static str>,
    lang: Lang,
}

impl Rules {
    pub fn new(lang: Lang) -> Rules {
        let (kw, ty, bi): (Vec<&str>, &str, &str) = match lang {
            Lang::C => (vec![C_KW], C_TYPES, ""),
            Lang::Cpp => (vec![C_KW, CPP_KW], C_TYPES, ""),
            Lang::Java => (vec![JAVA_KW], "String Object Integer Double Boolean Character Long Math System", ""),
            Lang::CSharp => (vec![CS_KW], "String Console List Dictionary Task Math", ""),
            Lang::Js => (vec![JS_KW], "", "console document window Math JSON Promise Array Object"),
            Lang::Ts => (vec![JS_KW, TS_KW], "", "console document window Math JSON Promise Array Object"),
            Lang::Rust => (vec![RUST_KW], RUST_TYPES, ""),
            Lang::Go => (vec![GO_KW], GO_TYPES, ""),
            Lang::Kotlin => (vec![KT_KW], "Int Long Double Float String Boolean Char Unit Any List Map Set", ""),
            Lang::Python => (vec![PY_KW], "", PY_BUILTINS),
            Lang::Shell | Lang::Make => (vec![SH_KW], "", ""),
            Lang::PowerShell => (vec![PS_KW], "", ""),
            Lang::Sql => (vec![SQL_KW], "", ""),
            Lang::Lua => (vec![LUA_KW], "", ""),
            Lang::Php => (vec![PHP_KW], "", ""),
            Lang::Json | Lang::Yaml | Lang::Toml => (vec!["true false null yes no on off"], "", ""),
            _ => (vec![], "", ""),
        };
        Rules {
            keywords: kw.iter().flat_map(|s| s.split_whitespace()).collect(),
            types: ty.split_whitespace().collect(),
            builtins: bi.split_whitespace().collect(),
            lang,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind { Text, Keyword, Type, Func, Const, Number, Str, Comment, Prepro, Include, Op, Decorator, Heading, Tag }

fn color(k: Kind, p: &Palette) -> Color32 {
    match k {
        Kind::Text => p.fg,
        Kind::Keyword => p.magenta,
        Kind::Type => p.yellow,
        Kind::Func => p.blue,
        Kind::Const | Kind::Number => p.orange,
        Kind::Str | Kind::Include => p.green,
        Kind::Comment => p.comment,
        Kind::Prepro => p.red,
        Kind::Op => p.cyan,
        Kind::Decorator => p.yellow,
        Kind::Heading => p.blue,
        Kind::Tag => p.red,
    }
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c >= 0x80
}
fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}
fn all_caps(w: &str) -> bool {
    w.len() >= 2 && w.bytes().any(|c| c.is_ascii_uppercase())
        && w.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
}

/// Premier passage : noms de fonctions (suivis de « ( ») et de types (déclarations).
fn collect_names(text: &[u8], rules: &Rules) -> (HashSet<String>, HashSet<String>) {
    let mut funcs = HashSet::new();
    let mut types = HashSet::new();
    let mut words: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < text.len() {
        if is_ident_start(text[i]) && (i == 0 || !is_ident(text[i - 1])) {
            let s = i;
            while i < text.len() && is_ident(text[i]) {
                i += 1;
            }
            words.push((s, i));
        } else {
            i += 1;
        }
    }
    let word = |k: usize| std::str::from_utf8(&text[words[k].0..words[k].1]).unwrap_or("");
    let next_char = |pos: usize| text[pos..].iter().copied().find(|c| !c.is_ascii_whitespace());
    for k in 0..words.len() {
        let w = word(k);
        let (_, e) = words[k];
        if rules.keywords.contains(w) {
            // « struct Nom », « class Nom », « enum Nom » : déclaration de type
            if matches!(w, "struct" | "class" | "enum" | "union" | "interface" | "record" | "trait" | "type")
                && k + 1 < words.len() {
                types.insert(word(k + 1).to_string());
            }
            continue;
        }
        let nc = next_char(e);
        if nc == Some(b'(') {
            funcs.insert(w.to_string());
            continue;
        }
        if rules.lang.brace_family() && !all_caps(w) {
            // « Type* nom », « Type nom; », « Type nom = », « Type nom) »
            let mut j = e;
            let mut pointer = false;
            while j < text.len() && matches!(text[j], b' ' | b'\t' | b'*' | b'&') {
                pointer |= matches!(text[j], b'*' | b'&');
                j += 1;
            }
            if j > e && j < text.len() && k + 1 < words.len() && words[k + 1].0 == j {
                let after = next_char(words[k + 1].1);
                let next = word(k + 1);
                let likely = w.as_bytes()[0].is_ascii_uppercase() || w.ends_with("_t");
                // « a * b; » est une multiplication : avec * ou &, il faut un nom qui ressemble à un type
                if !rules.keywords.contains(next) && (likely || !pointer)
                    && matches!(after, Some(b';' | b'=' | b',' | b')' | b'[' | b'(')) {
                    types.insert(w.to_string());
                }
            }
        }
    }
    // typedef ... Nom ; et « } Nom ; » après un typedef struct
    if matches!(rules.lang, Lang::C | Lang::Cpp) {
        let s = String::from_utf8_lossy(text);
        for part in s.split("typedef").skip(1) {
            // typedef struct { … } Nom;  → le nom est après l'accolade fermante
            let part = match (part.find('{'), part.find(';')) {
                (Some(o), Some(sc)) if o < sc => {
                    let mut depth = 0;
                    let mut close = part.len();
                    for (idx, ch) in part.char_indices().skip_while(|(i, _)| *i < o) {
                        if ch == '{' { depth += 1; }
                        if ch == '}' { depth -= 1; if depth == 0 { close = idx; break; } }
                    }
                    &part[close.min(part.len())..]
                }
                _ => part,
            };
            if let Some(end) = part.find(';') {
                let decl = &part[..end];
                let name: String = decl.chars().rev().skip_while(|c| !(c.is_alphanumeric() || *c == '_'))
                    .take_while(|c| c.is_alphanumeric() || *c == '_').collect::<Vec<_>>().into_iter().rev().collect();
                if !name.is_empty() {
                    types.insert(name);
                }
            }
        }
    }
    for t in &types {
        funcs.remove(t);
    }
    (funcs, types)
}

/// Découpe le texte en morceaux colorés.
pub fn highlight(text: &str, lang: Lang, p: &Palette, font: &FontId) -> LayoutJob {
    let mut job = LayoutJob::default();
    let b = text.as_bytes();
    let rules = Rules::new(lang);
    let (funcs, decl_types) = if text.len() < 800_000 { collect_names(b, &rules) } else { Default::default() };
    let (line_c, block_open, block_close) = lang.comments();
    let push = |job: &mut LayoutJob, s: usize, e: usize, k: Kind| {
        if e > s {
            let mut f = TextFormat::simple(font.clone(), color(k, p));
            f.italics = k == Kind::Comment;
            job.append(&text[s..e], 0.0, f);
        }
    };
    let n = b.len();
    let mut i = 0;
    let mut line_start = true;
    let mut prepro_line = false;
    let mut include_line = false;
    let mut md_code = false;
    while i < n {
        let c = b[i];
        let starts = |pat: &str| !pat.is_empty() && b[i..].starts_with(pat.as_bytes());
        if c == b'\n' {
            push(&mut job, i, i + 1, Kind::Text);
            i += 1;
            line_start = true;
            prepro_line = false;
            include_line = false;
            continue;
        }
        // Markdown : titres, blocs de code
        if lang == Lang::Markdown {
            let end = b[i..].iter().position(|&x| x == b'\n').map(|x| x + i).unwrap_or(n);
            let line = &text[i..end];
            let k = if line.starts_with("```") {
                md_code = !md_code;
                Kind::Comment
            } else if md_code {
                Kind::Str
            } else if line_start && line.starts_with('#') {
                Kind::Heading
            } else if line_start && (line.trim_start().starts_with("- ") || line.trim_start().starts_with("* ")) {
                Kind::Decorator
            } else {
                Kind::Text
            };
            push(&mut job, i, end, k);
            i = end;
            line_start = false;
            continue;
        }
        // commentaires
        if starts(block_open) {
            let rest = &text[i + block_open.len()..];
            let end = rest.find(block_close).map(|x| i + block_open.len() + x + block_close.len()).unwrap_or(n);
            push(&mut job, i, end, Kind::Comment);
            i = end;
            line_start = false;
            continue;
        }
        if starts(line_c) && !(lang == Lang::Shell && i > 0 && b[i - 1] == b'$') {
            let end = b[i..].iter().position(|&x| x == b'\n').map(|x| x + i).unwrap_or(n);
            push(&mut job, i, end, Kind::Comment);
            i = end;
            continue;
        }
        if c == b' ' || c == b'\t' || c == b'\r' {
            let s = i;
            while i < n && (b[i] == b' ' || b[i] == b'\t' || b[i] == b'\r') {
                i += 1;
            }
            push(&mut job, s, i, Kind::Text);
            continue;
        }
        // préprocesseur C / C++
        if c == b'#' && line_start && matches!(lang, Lang::C | Lang::Cpp | Lang::CSharp) {
            let s = i;
            i += 1;
            while i < n && (b[i] == b' ' || b[i] == b'\t') {
                i += 1;
            }
            while i < n && is_ident(b[i]) {
                i += 1;
            }
            include_line = text[s..i].contains("include") || text[s..i].contains("import");
            prepro_line = true;
            push(&mut job, s, i, Kind::Prepro);
            line_start = false;
            continue;
        }
        if include_line && c == b'<' {
            let end = b[i..].iter().position(|&x| x == b'>' || x == b'\n').map(|x| x + i + 1).unwrap_or(n);
            push(&mut job, i, end.min(n), Kind::Include);
            i = end.min(n);
            continue;
        }
        let _ = prepro_line;
        // décorateurs / annotations
        if c == b'@' && matches!(lang, Lang::Python | Lang::Java | Lang::Kotlin | Lang::CSharp | Lang::Ts) {
            let s = i;
            i += 1;
            while i < n && (is_ident(b[i]) || b[i] == b'.') {
                i += 1;
            }
            push(&mut job, s, i, Kind::Decorator);
            line_start = false;
            continue;
        }
        // HTML / XML : balises
        if lang == Lang::Html && c == b'<' {
            let end = b[i..].iter().position(|&x| x == b'>').map(|x| x + i + 1).unwrap_or(n);
            let tag = &text[i..end];
            let name_end = tag[1..].find(|ch: char| ch.is_whitespace() || ch == '>' || ch == '/' && false)
                .map(|x| x + 1).unwrap_or(tag.len());
            push(&mut job, i, i + name_end, Kind::Tag);
            // attributs et valeurs
            let mut j = i + name_end;
            while j < end {
                if b[j] == b'"' || b[j] == b'\'' {
                    let q = b[j];
                    let e = b[j + 1..end].iter().position(|&x| x == q).map(|x| x + j + 2).unwrap_or(end);
                    push(&mut job, j, e, Kind::Str);
                    j = e;
                } else {
                    let s = j;
                    while j < end && b[j] != b'"' && b[j] != b'\'' {
                        j += 1;
                    }
                    push(&mut job, s, j, Kind::Decorator);
                }
            }
            i = end;
            line_start = false;
            continue;
        }
        // chaînes
        if c == b'"' || c == b'\'' || (c == b'`' && matches!(lang, Lang::Js | Lang::Ts | Lang::Go | Lang::Shell)) {
            let s = i;
            // en Rust, ' est souvent une durée de vie : 'a
            if c == b'\'' && lang == Lang::Rust && !(i + 2 < n && (b[i + 2] == b'\'' || b[i + 1] == b'\\')) {
                i += 1;
                while i < n && is_ident(b[i]) {
                    i += 1;
                }
                push(&mut job, s, i, Kind::Type);
                continue;
            }
            let triple = lang == Lang::Python && b[i..].starts_with(if c == b'"' { b"\"\"\"" } else { b"'''" });
            if triple {
                let pat = if c == b'"' { "\"\"\"" } else { "'''" };
                let end = text[i + 3..].find(pat).map(|x| i + 3 + x + 3).unwrap_or(n);
                push(&mut job, s, end, Kind::Str);
                i = end;
                continue;
            }
            i += 1;
            while i < n && b[i] != c && b[i] != b'\n' {
                if b[i] == b'\\' && i + 1 < n {
                    i += 1;
                }
                i += 1;
            }
            if i < n && b[i] == c {
                i += 1;
            }
            push(&mut job, s, i, Kind::Str);
            line_start = false;
            continue;
        }
        // nombres
        if c.is_ascii_digit() || (c == b'.' && i + 1 < n && b[i + 1].is_ascii_digit() && (i == 0 || !is_ident(b[i - 1]))) {
            let s = i;
            while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') {
                i += 1;
            }
            push(&mut job, s, i, Kind::Number);
            line_start = false;
            continue;
        }
        // identifiants
        if is_ident_start(c) {
            let s = i;
            while i < n && is_ident(b[i]) {
                i += 1;
            }
            // variables shell $NOM, ${NOM}
            let w = &text[s..i];
            let k = if lang == Lang::Css {
                if b[i..].iter().copied().find(|x| *x != b' ') == Some(b':') { Kind::Func } else { Kind::Text }
            } else if matches!(lang, Lang::Json | Lang::Yaml | Lang::Toml) {
                if rules.keywords.contains(w) { Kind::Keyword } else { Kind::Text }
            } else if rules.keywords.contains(w) || (lang == Lang::Sql && rules.keywords.contains(w.to_lowercase().as_str())) {
                Kind::Keyword
            } else if rules.types.contains(w) || decl_types.contains(w)
                || (lang.camel_types() && w.as_bytes()[0].is_ascii_uppercase() && w.bytes().any(|x| x.is_ascii_lowercase())) {
                Kind::Type
            } else if all_caps(w) || matches!(w, "NULL" | "nullptr" | "true" | "false" | "None" | "True" | "False") {
                Kind::Const
            } else if funcs.contains(w) || rules.builtins.contains(w) {
                Kind::Func
            } else {
                Kind::Text
            };
            push(&mut job, s, i, k);
            line_start = false;
            continue;
        }
        if c == b'$' && matches!(lang, Lang::Shell | Lang::Make | Lang::PowerShell | Lang::Php) {
            let s = i;
            i += 1;
            if i < n && (b[i] == b'{' || b[i] == b'(') {
                let close = if b[i] == b'{' { b'}' } else { b')' };
                while i < n && b[i] != close && b[i] != b'\n' {
                    i += 1;
                }
                i = (i + 1).min(n);
            } else {
                while i < n && (is_ident(b[i]) || b[i] == b'@' || b[i] == b'?') {
                    i += 1;
                }
            }
            push(&mut job, s, i, Kind::Const);
            continue;
        }
        // opérateurs et ponctuation (UTF-8 : on avance d'un caractère entier)
        let len = utf8_len(c);
        let k = if b"+-*/%=<>!&|^~?:".contains(&c) { Kind::Op } else { Kind::Text };
        push(&mut job, i, (i + len).min(n), k);
        i += len;
        line_start = false;
    }
    job
}

fn utf8_len(c: u8) -> usize {
    match c {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str, lang: Lang) -> Vec<(String, Color32)> {
        let p = crate::theme::palette("Tokyo Night");
        let job = highlight(src, lang, &p, &FontId::monospace(13.0));
        job.sections.iter().map(|s| (src[s.byte_range.start.0..s.byte_range.end.0].to_string(), s.format.color)).collect()
    }

    fn col(v: &[(String, Color32)], w: &str) -> Color32 {
        v.iter().find(|(s, _)| s.trim() == w).unwrap_or_else(|| panic!("{w} absent")).1
    }

    #[test]
    fn c_semantic() {
        let p = crate::theme::palette("Tokyo Night");
        let src = "#include \"raylib.h\"\n#include <stdio.h>\n#define SIZEMAP 100\n// note\n\
            void loadMap(Model* tab, size_t size, Mesh mesh_cube) {\n  for(size_t i = 0; i < size; i++) \
            { tab[i] = LoadModelFromMesh(mesh_cube); x = MATERIAL_MAP_DIFFUSE; }\n}\n";
        let v = kinds(src, Lang::C);
        assert_eq!(col(&v, "#include"), p.red);
        assert_eq!(col(&v, "<stdio.h>"), p.green);
        assert_eq!(col(&v, "\"raylib.h\""), p.green);
        assert_eq!(col(&v, "SIZEMAP"), p.orange);
        assert_eq!(col(&v, "void"), p.magenta);
        assert_eq!(col(&v, "loadMap"), p.blue);
        assert_eq!(col(&v, "Model"), p.yellow);
        assert_eq!(col(&v, "size_t"), p.yellow);
        assert_eq!(col(&v, "Mesh"), p.yellow);
        assert_eq!(col(&v, "LoadModelFromMesh"), p.blue);
        assert_eq!(col(&v, "MATERIAL_MAP_DIFFUSE"), p.orange);
        assert_eq!(col(&v, "// note"), p.comment);
        // le texte est reconstitué à l'identique
        let job = highlight(src, Lang::C, &p, &FontId::monospace(13.0));
        assert_eq!(job.text, src);
    }

    #[test]
    fn python_and_java() {
        let p = crate::theme::palette("Tokyo Night");
        let v = kinds("class Message:\n    def code(self, x):\n        print(len(x))  # c\n        s = \"\"\"doc\"\"\"\n", Lang::Python);
        assert_eq!(col(&v, "class"), p.magenta);
        assert_eq!(col(&v, "Message"), p.yellow);
        assert_eq!(col(&v, "code"), p.blue);
        assert_eq!(col(&v, "print"), p.blue);
        assert_eq!(col(&v, "# c"), p.comment);
        assert_eq!(col(&v, "\"\"\"doc\"\"\""), p.green);
        let v = kinds("public class App { static final int MAX = 4; void run(String s) { s.length(); } }", Lang::Java);
        assert_eq!(col(&v, "App"), p.yellow);
        assert_eq!(col(&v, "String"), p.yellow);
        assert_eq!(col(&v, "MAX"), p.orange);
        assert_eq!(col(&v, "length"), p.blue);
    }

    #[test]
    fn big_file_is_fast() {
        let p = crate::theme::palette("Tokyo Night");
        let unit = "void f(Model* m, size_t n) { for (size_t i = 0; i < n; i++) { m[i] = LoadModel(\"x\"); } } // ok\n";
        let src = unit.repeat(5000);
        let t = std::time::Instant::now();
        let job = highlight(&src, Lang::C, &p, &FontId::monospace(13.0));
        let ms = t.elapsed().as_millis();
        eprintln!("5000 lignes : {ms} ms");
        assert_eq!(job.text.len(), src.len());
        assert!(ms < 1500);
    }

    #[test]
    fn utf8_text_kept() {
        let p = crate::theme::palette("Tokyo Night");
        let src = "// é à ç\nint x = 1; // ✓\nchar *s = \"déjà\";\n";
        let job = highlight(src, Lang::C, &p, &FontId::monospace(13.0));
        assert_eq!(job.text, src);
    }
}
