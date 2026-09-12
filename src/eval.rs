use std::collections::HashMap;

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

pub fn eval_truth(expr: &str, vars: &HashMap<String, String>) -> bool {
    let e = expr.trim();
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
    if let Some(rest) = e.strip_prefix("not ").or_else(|| e.strip_prefix('!')) {
        return !eval_truth(rest.trim(), vars);
    }
    for (sep, op) in [
        (" and ", "and"),
        (" && ", "and"),
        (" or ", "or"),
        (" || ", "or"),
    ] {
        if let Some((a, b)) = e.split_once(sep) {
            let la = eval_truth(a, vars);
            let lb = eval_truth(b, vars);
            return if op == "and" { la && lb } else { la || lb };
        }
    }
    for op in [">=", "<=", "!=", "==", ">", "<"] {
        if let Some((a, b)) = e.split_once(op) {
            let av = lookup(a.trim(), vars);
            let bv = lookup(b.trim(), vars);
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
    }
}
