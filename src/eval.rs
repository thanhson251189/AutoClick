use std::collections::HashMap;

use chrono::{Datelike, Timelike};

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
    if let Some(v) = try_call(expr, vars) {
        return v.trim().parse().ok();
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

/// Variable copy, `{name}` expansion, `+ - * /`, or `FUNC(args)` calls.
/// A value that is only a variable name is copied as stored, so `hello-world` stays text.
pub fn eval_value(expr: &str, vars: &HashMap<String, String>) -> String {
    let trimmed = expr.trim();
    if let Some(result) = try_call(trimmed, vars) {
        return result;
    }
    let expanded = expand_text(trimmed, vars);
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
    let trimmed = expr.trim();
    if let Some(result) = try_call(trimmed, vars) {
        return truthy(&result);
    }
    let expanded = expand_text(trimmed, vars);
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

/// Match a fully wrapped `NAME(args)` where NAME is a known function: the first
/// `(` must balance exactly at the final character, so `LEN(a) + 1` is not a call.
fn fn_name_at(expr: &str) -> Option<(String, &str)> {
    let open = expr.find('(')?;
    let name = expr[..open].trim();
    let mut chars = name.chars();
    if !chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    if !expr.ends_with(')') {
        return None;
    }
    let inner = &expr[open + 1..expr.len() - 1];
    let mut depth = 0i32;
    let mut in_quote = false;
    for &b in inner.as_bytes() {
        match b {
            b'"' => in_quote = !in_quote,
            b'(' if !in_quote => depth += 1,
            b')' if !in_quote => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
    }
    if depth != 0 || in_quote {
        return None;
    }
    Some((name.to_ascii_uppercase(), inner))
}

/// Split on top-level commas, ignoring commas inside quotes:
/// `a, MIN(1, 2), "x,y"` -> `["a", "MIN(1, 2)", "\"x,y\""]`.
fn split_args(expr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut start = 0usize;
    for (i, &b) in expr.as_bytes().iter().enumerate() {
        match b {
            b'"' => in_quote = !in_quote,
            b'(' if !in_quote => depth += 1,
            b')' if !in_quote => depth -= 1,
            b',' if depth == 0 && !in_quote => {
                out.push(expr[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(expr[start..].trim().to_string());
    out
}

/// Evaluate a `FUNC(...)` expression. Quoted string arguments lose their quotes.
fn try_call(expr: &str, vars: &HashMap<String, String>) -> Option<String> {
    let (name, inner) = fn_name_at(expr)?;
    let args = if inner.trim().is_empty() {
        Vec::new()
    } else {
        split_args(inner)
    };
    let mut vals = Vec::with_capacity(args.len());
    for arg in &args {
        let arg = arg.trim();
        let arg = if arg.len() >= 2 && arg.starts_with('"') && arg.ends_with('"') {
            &arg[1..arg.len() - 1]
        } else {
            arg
        };
        vals.push(eval_value(arg, vars));
    }
    call_fn(&name, &vals, vars)
}

fn num_arg(args: &[String], i: usize) -> Option<f64> {
    args.get(i).and_then(|s| s.trim().parse::<f64>().ok())
}

/// AMK-style function library. Names are case-insensitive; string position
/// arguments are 1-based, `INSTR`/`RAND`-style failures return 0 or the low end.
fn call_fn(name: &str, args: &[String], _vars: &HashMap<String, String>) -> Option<String> {
    let s0 = || args.first().map(String::as_str).unwrap_or("");
    let n0 = |i: usize| num_arg(args, i);
    Some(match name {
        "LEN" => s0().chars().count().to_string(),
        "UPPER" => s0().to_uppercase(),
        "LOWER" => s0().to_lowercase(),
        "TRIM" => s0().trim().to_string(),
        "MID" => {
            let chars: Vec<char> = s0().chars().collect();
            let start = (n0(1)?.max(1.0) as usize).min(chars.len().max(1));
            let rest = &chars[start - 1..];
            match n0(2) {
                Some(len) => rest.iter().take(len.max(0.0) as usize).collect(),
                None => rest.iter().collect(),
            }
        }
        "LEFT" => s0().chars().take(n0(1)?.max(0.0) as usize).collect(),
        "RIGHT" => {
            let n = n0(1)?.max(0.0) as usize;
            let count = s0().chars().count();
            s0().chars().skip(count.saturating_sub(n)).collect()
        }
        "REPLACE" => s0().replace(args.get(1)?.as_str(), args.get(2)?.as_str()),
        "INSTR" => {
            let hay = s0();
            match hay.find(args.get(1)?.as_str()) {
                Some(byte_i) => (hay[..byte_i].chars().count() + 1).to_string(),
                None => "0".into(),
            }
        }
        "VAL" => s0()
            .trim()
            .parse::<f64>()
            .map(format_num)
            .unwrap_or("0".into()),
        "STR" => n0(0).map(format_num).unwrap_or_else(|| s0().to_string()),
        "ASC" => s0()
            .chars()
            .next()
            .map(|c| (c as u32).to_string())
            .unwrap_or("0".into()),
        "CHR" => char::from_u32(n0(0)? as u32)?.to_string(),
        "ABS" => format_num(n0(0)?.abs()),
        "SQRT" => format_num(n0(0)?.sqrt()),
        "CEIL" => format_num(n0(0)?.ceil()),
        "FLOOR" => format_num(n0(0)?.floor()),
        "ROUND" => {
            let m = 10f64.powi(n0(1).unwrap_or(0.0).clamp(-6.0, 6.0) as i32);
            format_num((n0(0)? * m).round() / m)
        }
        "POW" => format_num(n0(0)?.powf(n0(1)?)),
        "MOD" => format_num(n0(0)? % n0(1)?),
        "MIN" => format_num(
            args.iter()
                .filter_map(|s| s.trim().parse::<f64>().ok())
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))?,
        ),
        "MAX" => format_num(
            args.iter()
                .filter_map(|s| s.trim().parse::<f64>().ok())
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))?,
        ),
        "RAND" => crate::engine::rand_range(n0(0)? as i64, n0(1)? as i64).to_string(),
        "NOW" => chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "TODAY" => chrono::Local::now().format("%Y-%m-%d").to_string(),
        "YEAR" => chrono::Local::now().year().to_string(),
        "MONTH" => chrono::Local::now().month().to_string(),
        "DAY" => chrono::Local::now().day().to_string(),
        "HOUR" => chrono::Local::now().hour().to_string(),
        "MINUTE" => chrono::Local::now().minute().to_string(),
        "SECOND" => chrono::Local::now().second().to_string(),
        "WEEKDAY" => chrono::Local::now()
            .weekday()
            .number_from_monday()
            .to_string(),
        "TICKS" => chrono::Utc::now().timestamp_millis().to_string(),
        "FILE_EXISTS" => i8::from(std::path::Path::new(s0()).exists()).to_string(),
        "FILE_SIZE" => std::fs::metadata(s0())
            .map(|m| m.len().to_string())
            .unwrap_or("-1".into()),
        "FILE_READ" => {
            // Whole-text helper for small files; oversized files read as "".
            match std::fs::read(s0()) {
                Ok(bytes) if bytes.len() <= 1 << 20 => String::from_utf8_lossy(&bytes).into_owned(),
                _ => String::new(),
            }
        }
        "FILE_WRITE" => {
            if std::fs::write(s0(), args.get(1).map(String::as_str).unwrap_or("")).is_ok() {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        "ENV" => std::env::var(s0()).unwrap_or_default(),
        "CLIP" => crate::clipboard::get_text().unwrap_or_default(),
        _ => return None,
    })
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

    #[test]
    fn string_functions() {
        let empty = HashMap::new();
        assert_eq!(eval_value("LEN(abc)", &empty), "3");
        assert_eq!(eval_value("upper(ab)", &empty), "AB");
        assert_eq!(eval_value("LOWER(AB)", &empty), "ab");
        assert_eq!(eval_value("TRIM(  ab  )", &empty), "ab");
        assert_eq!(eval_value(r#"MID("abcdef", 2, 3)"#, &empty), "bcd");
        assert_eq!(eval_value("MID(abcdef, 4)", &empty), "def");
        assert_eq!(eval_value("LEFT(abcdef, 2)", &empty), "ab");
        assert_eq!(eval_value("RIGHT(abcdef, 2)", &empty), "ef");
        assert_eq!(eval_value(r#"REPLACE(abcabc, bc, X)"#, &empty), "aXaX");
        assert_eq!(eval_value(r#"INSTR(abcdef, cd)"#, &empty), "3");
        assert_eq!(eval_value(r#"INSTR(abcdef, zz)"#, &empty), "0");
        assert_eq!(eval_value("VAL(3.50)", &empty), "3.5");
        assert_eq!(eval_value("STR(3.50)", &empty), "3.5");
        assert_eq!(eval_value("ASC(a)", &empty), "97");
        assert_eq!(eval_value("CHR(97)", &empty), "a");
    }

    #[test]
    fn math_functions() {
        let empty = HashMap::new();
        assert_eq!(eval_value("ABS(-3)", &empty), "3");
        assert_eq!(eval_value("SQRT(16)", &empty), "4");
        assert_eq!(eval_value("SQRT(16) + 1", &empty), "5");
        assert_eq!(eval_value("CEIL(1.2)", &empty), "2");
        assert_eq!(eval_value("FLOOR(1.8)", &empty), "1");
        assert_eq!(eval_value("ROUND(1.25, 1)", &empty), "1.3");
        assert_eq!(eval_value("POW(2, 3)", &empty), "8");
        assert_eq!(eval_value("MOD(7, 4)", &empty), "3");
        assert_eq!(eval_value("MIN(3, 1, 2)", &empty), "1");
        assert_eq!(eval_value("MAX(3, 1, 2)", &empty), "3");
        let v = vars(&[("n", "9")]);
        assert_eq!(eval_value("SQRT(n)", &v), "3");
    }

    #[test]
    fn rand_and_time_functions() {
        let empty = HashMap::new();
        let drawn = eval_value("RAND(5, 5)", &empty);
        assert_eq!(drawn, "5");
        for _ in 0..20 {
            let n: i64 = eval_value("RAND(1, 3)", &empty).parse().unwrap();
            assert!((1..=3).contains(&n));
        }
        assert_eq!(eval_value("LEN(NOW())", &empty), "19");
        assert_eq!(eval_value("LEN(TODAY())", &empty), "10");
        let year: i64 = eval_value("YEAR()", &empty).parse().unwrap();
        assert!(year > 2020);
        let wd: i64 = eval_value("WEEKDAY()", &empty).parse().unwrap();
        assert!((1..=7).contains(&wd));
    }

    #[test]
    fn file_functions_roundtrip() {
        let empty = HashMap::new();
        let mut path = std::env::temp_dir();
        path.push(format!("amk-eval-{}.txt", std::process::id()));
        let p = path.to_string_lossy().replace('\\', "/");
        assert_eq!(eval_value(&format!("FILE_EXISTS({p})"), &empty), "0");
        assert_eq!(
            eval_value(&format!(r#"FILE_WRITE({p}, "hello fn")"#), &empty),
            "1"
        );
        assert_eq!(eval_value(&format!("FILE_EXISTS({p})"), &empty), "1");
        assert_eq!(eval_value(&format!("FILE_READ({p})"), &empty), "hello fn");
        assert!(
            eval_value(&format!("FILE_SIZE({p})"), &empty)
                .parse::<i64>()
                .unwrap()
                > 0
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn env_and_clip_functions_degrade_gracefully() {
        let empty = HashMap::new();
        assert_eq!(eval_value(r#"ENV(AMK_NO_SUCH_VAR_XYZ)"#, &empty), "");
        // Depends on machine state; the contract is only that it returns text.
        let _clip = eval_value("CLIP()", &empty);
    }

    #[test]
    fn nested_calls_and_quoted_args() {
        let empty = HashMap::new();
        assert_eq!(eval_value(r#"UPPER(TRIM(  ab  ))"#, &empty), "AB");
        assert_eq!(eval_value(r#"LEN(LEFT(abcdef, 3))"#, &empty), "3");
        assert_eq!(eval_value(r#"MID("a,b,c", 2, 1)"#, &empty), ",");
    }

    #[test]
    fn unknown_call_shape_stays_text() {
        let empty = HashMap::new();
        assert_eq!(eval_value("NOFUNC(1)", &empty), "NOFUNC(1)");
        assert_eq!(eval_value("LEN(a) + LEN(b)", &empty), "2");
    }

    #[test]
    fn functions_inside_truth() {
        let empty = HashMap::new();
        assert!(eval_truth("LEN(abc) == 3", &empty));
        assert!(eval_truth("MAX(1, 5) == 5", &empty));
        assert!(!eval_truth("INSTR(abc, z) > 0", &empty));
        assert!(eval_truth(r#"TRIM( x ) == x"#, &empty));
    }
}
