//! PowerPoint COM 探测（VBA spike）。纯函数跨平台可测；COM 部分仅 Windows。

/// 从一段 VBA 模块代码里抽出可调用的过程名（Sub / Function）。
/// 忽略大小写与前导空白；忽略 Private；去重保序（大小写不敏感去重）。
pub fn parse_macro_names(module_text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in module_text.lines() {
        let t = line.trim_start();
        let lower = t.to_ascii_lowercase();
        if lower.starts_with("private ") {
            continue;
        }
        let rest = lower.strip_prefix("public ").unwrap_or(&lower);
        let kw = if let Some(r) = rest.strip_prefix("sub ") {
            Some(r)
        } else {
            rest.strip_prefix("function ")
        };
        if let Some(r) = kw {
            let name_lower_len = r.find(|c: char| c == '(' || c.is_whitespace()).unwrap_or(r.len());
            if name_lower_len > 0 {
                let start = t.len() - rest.len() + (rest.len() - r.len());
                let name = &t[start..start + name_lower_len];
                if name.chars().all(|c| c.is_alphanumeric() || c == '_')
                    && !out.iter().any(|n| n.eq_ignore_ascii_case(name))
                {
                    out.push(name.to_string());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_subs_and_functions_dedup_skip_private() {
        let code = "\
Sub ShowAnswerA()\n\
  ' body\n\
End Sub\n\
Public Sub NextQ\n\
End Sub\n\
Private Sub Helper()\n\
End Sub\n\
Function Score(x)\n\
End Function\n\
sub showanswera()\n\
End Sub\n";
        assert_eq!(parse_macro_names(code), vec!["ShowAnswerA", "NextQ", "Score"]);
    }

    #[test]
    fn empty_text_yields_nothing() {
        assert!(parse_macro_names("").is_empty());
        assert!(parse_macro_names("' just a comment\nDim x").is_empty());
    }
}
