//! Macro retry policy: never rerun a macro whose body may already have executed.

#[derive(Debug)]
pub struct InvokeFailure {
    pub hresult: u32,
    pub detail: String,
}

pub fn run_with_fallback(
    mut run: impl FnMut(bool) -> Result<(), InvokeFailure>,
    mut report: impl FnMut(&str),
) -> Result<(), String> {
    let first = match run(true) {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };
    // Only a dispatch argument-count rejection proves that the macro did not run.
    // DISP_E_EXCEPTION (including VBA error 91), busy, and unknown errors must stop.
    if first.hresult != 0x8002000e {
        return Err(format!("{}；未自动重试，避免宏重复执行", first.detail));
    }
    report(&format!("带形状调用被参数数量校验拒绝：{}；回退无参调用", first.detail));
    run(false).map_err(|e| format!("带形状失败：{}；无参回退失败：{}", first.detail, e.detail))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_error_91_is_not_retried() {
        let mut calls = Vec::new();
        let result = run_with_fallback(|with_shape| {
            calls.push(with_shape);
            Err(InvokeFailure { hresult: 0x80020009, detail: "VBA 91: object not set".into() })
        }, |_| {});
        assert_eq!(calls, vec![true]);
        assert!(result.unwrap_err().contains("VBA 91"));
    }

    #[test]
    fn only_argument_count_rejection_allows_no_arg_retry() {
        let mut calls = Vec::new();
        let mut events = Vec::new();
        let result = run_with_fallback(|with_shape| {
            calls.push(with_shape);
            if with_shape {
                Err(InvokeFailure { hresult: 0x8002000e, detail: "bad argument count".into() })
            } else { Ok(()) }
        }, |event| events.push(event.to_string()));
        assert!(result.is_ok());
        assert_eq!(calls, vec![true, false]);
        assert!(!events.is_empty());
    }

    #[test]
    fn failed_fallback_preserves_both_errors() {
        let result = run_with_fallback(|with_shape| Err(InvokeFailure {
            hresult: if with_shape { 0x8002000e } else { 0x80020009 },
            detail: if with_shape { "bad count" } else { "VBA 91" }.into(),
        }), |_| {});
        let error = result.unwrap_err();
        assert!(error.contains("bad count") && error.contains("VBA 91"));
    }
}
