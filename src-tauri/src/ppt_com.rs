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

#[cfg(windows)]
mod com {
    use windows::core::{w, Interface, BSTR, GUID, IUnknown, PCWSTR, VARIANT};
    use windows::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, CLSIDFromProgID, COINIT_APARTMENTTHREADED, DISPATCH_FLAGS,
        DISPATCH_METHOD, DISPATCH_PROPERTYGET, DISPPARAMS, IDispatch,
    };
    use windows::Win32::System::Ole::GetActiveObject;

    const LOCALE_USER_DEFAULT: u32 = 0x0400;
    const IID_NULL: GUID = GUID::from_u128(0);

    /// 线程级 COM 初始化守卫（每次命令调用一次）。
    pub struct ComGuard {
        uninit: bool,
    }
    impl ComGuard {
        pub fn init() -> Self {
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            // 只有本次确实初始化成功（S_OK/S_FALSE）才在 Drop 里平衡 CoUninitialize；
            // RPC_E_CHANGED_MODE 表示线程已被别处初始化为不同公寓，本次未持有引用，不能 uninit。
            ComGuard { uninit: hr.is_ok() }
        }
    }
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.uninit {
                unsafe { CoUninitialize() };
            }
        }
    }

    /// 连接已运行的 PowerPoint.Application，返回其 IDispatch。
    pub fn connect() -> Result<IDispatch, String> {
        unsafe {
            let clsid = CLSIDFromProgID(w!("PowerPoint.Application"))
                .map_err(|e| format!("找不到 PowerPoint：{e}"))?;
            let mut unk: Option<IUnknown> = None;
            GetActiveObject(&clsid, None, &mut unk)
                .map_err(|_| "PowerPoint 未运行，请先打开 PowerPoint。".to_string())?;
            let unk = unk.ok_or_else(|| "GetActiveObject 返回空".to_string())?;
            unk.cast::<IDispatch>().map_err(|e| e.to_string())
        }
    }

    fn invoke(disp: &IDispatch, name: &str, flags: DISPATCH_FLAGS, args: &[VARIANT]) -> Result<VARIANT, String> {
        unsafe {
            let wname: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let names = [PCWSTR(wname.as_ptr())];
            let mut dispid: i32 = 0;
            disp.GetIDsOfNames(&IID_NULL, names.as_ptr(), 1, LOCALE_USER_DEFAULT, &mut dispid)
                .map_err(|e| format!("{name}: 找不到成员（{e}）"))?;
            let mut argv: Vec<VARIANT> = args.iter().rev().cloned().collect();
            let params = DISPPARAMS {
                rgvarg: if argv.is_empty() { std::ptr::null_mut() } else { argv.as_mut_ptr() },
                cArgs: argv.len() as u32,
                ..Default::default()
            };
            let mut result = VARIANT::default();
            disp.Invoke(dispid, &IID_NULL, LOCALE_USER_DEFAULT, flags, &params, Some(&mut result), None, None)
                .map_err(|e| format!("{name}: 调用失败（{e}）"))?;
            Ok(result)
        }
    }

    pub fn get_obj(disp: &IDispatch, name: &str) -> Result<IDispatch, String> {
        let v = invoke(disp, name, DISPATCH_PROPERTYGET, &[])?;
        IDispatch::try_from(&v).map_err(|e| format!("{name} 不是对象（{e}）"))
    }

    pub fn get_i32(disp: &IDispatch, name: &str) -> Result<i32, String> {
        let v = invoke(disp, name, DISPATCH_PROPERTYGET, &[])?;
        i32::try_from(&v).map_err(|e| format!("{name} 不是整数（{e}）"))
    }

    pub fn call_obj(disp: &IDispatch, name: &str, args: &[VARIANT]) -> Result<IDispatch, String> {
        let v = invoke(disp, name, DISPATCH_METHOD | DISPATCH_PROPERTYGET, args)?;
        IDispatch::try_from(&v).map_err(|e| format!("{name} 未返回对象（{e}）"))
    }

    pub fn call_str(disp: &IDispatch, name: &str, args: &[VARIANT]) -> Result<String, String> {
        let v = invoke(disp, name, DISPATCH_METHOD | DISPATCH_PROPERTYGET, args)?;
        Ok(BSTR::try_from(&v).map_err(|e| format!("{name} 未返回字符串（{e}）"))?.to_string())
    }

    pub fn call_void(disp: &IDispatch, name: &str, args: &[VARIANT]) -> Result<(), String> {
        invoke(disp, name, DISPATCH_METHOD, args).map(|_| ())
    }
}

/// 列出当前活动演示文稿里可调用的 VBA 宏名。
#[cfg(windows)]
pub fn list_macros() -> Result<Vec<String>, String> {
    let _co = com::ComGuard::init();
    let app = com::connect()?;
    let pres = com::get_obj(&app, "ActivePresentation")
        .map_err(|_| "没有活动演示文稿（请打开一个 .pptm）".to_string())?;
    let vbproj = com::get_obj(&pres, "VBProject").map_err(|_| {
        "无法访问 VBProject：请在 PowerPoint 信任中心开启'信任对 VBA 工程对象模型的访问'。".to_string()
    })?;
    let comps = com::get_obj(&vbproj, "VBComponents")?;
    let count = com::get_i32(&comps, "Count")?;
    let mut names: Vec<String> = Vec::new();
    for i in 1..=count {
        let comp = com::call_obj(&comps, "Item", &[windows::core::VARIANT::from(i)])?;
        let code = com::get_obj(&comp, "CodeModule")?;
        let lines = com::get_i32(&code, "CountOfLines")?;
        if lines > 0 {
            let text = com::call_str(
                &code,
                "Lines",
                &[windows::core::VARIANT::from(1i32), windows::core::VARIANT::from(lines)],
            )?;
            for n in parse_macro_names(&text) {
                if !names.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
                    names.push(n);
                }
            }
        }
    }
    Ok(names)
}

#[cfg(not(windows))]
pub fn list_macros() -> Result<Vec<String>, String> {
    Err("仅 Windows 支持 PowerPoint COM".to_string())
}

/// 触发一个 VBA 宏（Application.Run）。
#[cfg(windows)]
pub fn run_macro(name: &str) -> Result<(), String> {
    let _co = com::ComGuard::init();
    let app = com::connect()?;
    com::call_void(&app, "Run", &[windows::core::VARIANT::from(name)])
}

#[cfg(not(windows))]
pub fn run_macro(_name: &str) -> Result<(), String> {
    Err("仅 Windows 支持 PowerPoint COM".to_string())
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
