use std::collections::HashMap;

fn split_ci<'a>(e: &'a str, sep: &str) -> Option<(&'a str, &'a str)> {
    let el = e.to_ascii_lowercase();
    let sl = sep.to_ascii_lowercase();
    let i = el.find(&sl)?;
    Some((e[..i].trim(), e[i + sep.len()..].trim()))
}

fn truthy(v: &str) -> bool {
    let s = v.trim();
    if s.is_empty() {
        return false;
    }
    let low = s.to_ascii_lowercase();
    !matches!(low.as_str(), "0" | "false" | "notfound" | "no" | "off")
}

fn lookup(expr: &str, vars: &HashMap<String, String>) -> String {
    if let Some(v) = vars.get(expr) {
        return v.clone();
    }
    if expr.eq_ignore_ascii_case("found") {
        return vars
            .get("found")
            .cloned()
            .unwrap_or_else(|| "notfound".into());
    }
    expr.to_string()
}

/// Replace `{name}` with that variable when it exists. Unknown braces stay put.
pub fn expand_text(text: &str, vars: &HashMap<String, String>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            if let Some(rel) = chars[i + 1..].iter().position(|c| *c == '}') {
                let name: String = chars[i + 1..i + 1 + rel].iter().collect();
                if let Some(v) = vars.get(name.trim()) {
                    out.push_str(v);
                    i += rel + 2;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn is_unary_at(bytes: &[u8], index: usize) -> bool {
    let mut j = index;
    while j > 0 && bytes[j - 1] == b' ' {
        j -= 1;
    }
    j == 0 || matches!(bytes[j - 1], b'+' | b'-' | b'*' | b'/' | b'(')
}

/// Rightmost `+`/`-` at paren depth 0, otherwise rightmost `*`/`/`.
fn split_lowest(expr: &str) -> Option<(&str, char, &str)> {
    let bytes = expr.as_bytes();
    let mut depth = 0i32;
    let mut add_at: Option<usize> = None;
    let mut mul_at: Option<usize> = None;
    for i in 0..bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'+' | b'-' if depth == 0 && !is_unary_at(bytes, i) => add_at = Some(i),
            b'*' | b'/' if depth == 0 && !is_unary_at(bytes, i) => mul_at = Some(i),
            _ => {}
        }
    }
    let i = add_at.or(mul_at)?;
    let op = bytes[i] as char;
    Some((expr[..i].trim(), op, expr[i + 1..].trim()))
}

fn strip_parens(expr: &str) -> Option<&str> {
    let bytes = expr.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'(' || bytes[bytes.len() - 1] != b')' {
        return None;
    }
    let mut depth = 0i32;
    for (i, c) in bytes.iter().enumerate() {
        match c {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 && i + 1 != bytes.len() {
                    return None;
                }
            }
            _ => {}
        }
    }
    if depth == 0 {
        Some(expr[1..expr.len() - 1].trim())
    } else {
        None
    }
}

fn atom(expr: &str, vars: &HashMap<String, String>) -> Option<f64> {
    let expr = expr.trim();
    if let Some(inner) = strip_parens(expr) {
        return arith(inner, vars);
    }
    let text = vars.get(expr).map(String::as_str).unwrap_or(expr);
    text.parse().ok()
}

fn arith(expr: &str, vars: &HashMap<String, String>) -> Option<f64> {
    let expr = expr.trim();
    if expr.is_empty() {
        return None;
    }
    if let Some((left, op, right)) = split_lowest(expr) {
        let a = arith(left, vars)?;
        let b = arith(right, vars)?;
        return Some(match op {
            '+' => a + b,
            '-' => a - b,
            '*' => a * b,
            '/' => {
                if b == 0.0 {
                    0.0
                } else {
                    a / b
                }
            }
            _ => return None,
        });
    }
    atom(expr, vars)
}

fn format_num(n: f64) -> String {
    if !n.is_finite() {
        return "0".into();
    }
    let rounded = n.round();
    if (n - rounded).abs() < 1e-9 && rounded.abs() < 1e15 {
        return format!("{}", rounded as i64);
    }
    let text = format!("{n:.8}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.to_string()
    }
}

/// Variable copy, `{name}` expansion, or `+ - * /` (parentheses and `*`/`/` bind tighter).
/// A value that is only a variable name is copied as stored, so `hello-world` stays text.
pub fn eval_value(expr: &str, vars: &HashMap<String, String>) -> String {
    let expanded = expand_text(expr.trim(), vars);
    let expanded = expanded.trim();
    if let Some(v) = vars.get(expanded) {
        return v.clone();
    }
    if split_lowest(expanded).is_some() {
        if let Some(n) = arith(expanded, vars) {
            return format_num(n);
        }
    }
    expanded.to_string()
}

pub fn eval_truth(expr: &str, vars: &HashMap<String, String>) -> bool {
    let expanded = expand_text(expr.trim(), vars);
    eval_truth_raw(expanded.trim(), vars)
}

fn eval_truth_raw(e: &str, vars: &HashMap<String, String>) -> bool {
    if e.is_empty() {
        return false;
    }
    let low = e.to_ascii_lowercase();
    if low == "true" || e == "1" {
        return true;
    }
    if low == "false" || e == "0" || low == "notfound" {
        return false;
    }
    if low.starts_with("not ") {
        return !eval_truth(e[4..].trim(), vars);
    }
    if let Some(rest) = e.strip_prefix('!') {
        return !eval_truth(rest.trim(), vars);
    }
    for (sep, op) in [
        (" and ", "and"),
        (" && ", "and"),
        (" or ", "or"),
        (" || ", "or"),
    ] {
        if let Some((a, b)) = split_ci(e, sep) {
            let la = eval_truth(a, vars);
            let lb = eval_truth(b, vars);
            return if op == "and" { la && lb } else { la || lb };
        }
    }
    for op in [">=", "<=", "!=", "==", ">", "<"] {
        if let Some((a, b)) = e.split_once(op) {
            let av = eval_value(a.trim(), vars);
            let bv = eval_value(b.trim(), vars);
            if let (Ok(x), Ok(y)) = (av.parse::<f64>(), bv.parse::<f64>()) {
                return match op {
                    ">" => x > y,
                    ">=" => x >= y,
                    "<" => x < y,
                    "<=" => x <= y,
                    "!=" => x != y,
                    _ => x == y,
                };
            }
            return match op {
                "!=" => av != bv,
                "==" => av == bv,
                _ => false,
            };
        }
    }
    truthy(&lookup(e, vars))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn found_default_is_false() {
        let vars = HashMap::new();
        assert!(!eval_truth("found", &vars));
    }

    #[test]
    fn notfound_token_is_false() {
        let mut vars = HashMap::new();
        vars.insert("found".into(), "notfound".into());
        assert!(!eval_truth("found", &vars));
        assert!(!eval_truth("notfound", &vars));
    }

    #[test]
    fn found_true_after_search() {
        let mut vars = HashMap::new();
        vars.insert("found".into(), "true".into());
        assert!(eval_truth("found", &vars));
        assert!(!eval_truth("not found", &vars));
    }

    #[test]
    fn comparisons_and_logic() {
        let mut vars = HashMap::new();
        vars.insert("n".into(), "4".into());
        vars.insert("found".into(), "true".into());
        assert!(eval_truth("n > 2", &vars));
        assert!(!eval_truth("n < 2", &vars));
        assert!(eval_truth("found and n == 4", &vars));
        assert!(eval_truth("n >= 4 and n <= 10", &vars));
        assert!(eval_truth("FOUND AND n == 4", &vars));
        assert!(!eval_truth("n == 1 OR n == 2", &vars));
        assert!(eval_truth("n == 4 OR n == 2", &vars));
    }

    #[test]
    fn braces_insert_variables_and_unknown_braces_stay() {
        let v = vars(&[("name", "Ada"), ("n", "3")]);
        assert_eq!(expand_text("Hello {name}", &v), "Hello Ada");
        assert_eq!(expand_text("{missing}", &v), "{missing}");
        assert_eq!(eval_value("{n}", &v), "3");
    }

    #[test]
    fn math_respects_precedence_parentheses_and_divide_by_zero() {
        let empty = HashMap::new();
        assert_eq!(eval_value("1 + 2 * 3", &empty), "7");
        assert_eq!(eval_value("(1 + 2) * 3", &empty), "9");
        assert_eq!(eval_value("5 / 2", &empty), "2.5");
        assert_eq!(eval_value("5 / 0", &empty), "0");
        let v = vars(&[("n", "3")]);
        assert_eq!(eval_value("n + 1", &v), "4");
        assert_eq!(eval_value("n - 1", &v), "2");
        assert!(eval_truth("n + 1 > 3", &v));
        assert!(eval_truth("{n} == 3", &v));
    }

    #[test]
    fn plain_text_and_variable_copy_are_not_treated_as_math() {
        let empty = HashMap::new();
        assert_eq!(eval_value("hello-world", &empty), "hello-world");
        assert_eq!(eval_value("08", &empty), "08");
        let v = vars(&[("file", "hello-world"), ("n", "4")]);
        assert_eq!(eval_value("file", &v), "hello-world");
        assert_eq!(eval_value("n", &v), "4");
    }
}
