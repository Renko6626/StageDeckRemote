//! PowerPoint COM 探测（VBA spike）。纯函数跨平台可测；COM 部分仅 Windows。

use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct SlideAction {
    pub id: String,         // 形状名(Shape.Name),触发标识
    pub label: String,      // 显示:可见文字优先,空则形状名
    pub macro_name: String, // 绑定的宏名(显示/日志;触发时服务端重新读取)
}

#[derive(Serialize, Clone, Debug)]
pub struct SlideState {
    pub available: bool,
    pub busy: bool,
    pub in_show: bool,
    pub current: i32,
    pub total: i32,
    pub actions: Vec<SlideAction>,
}

impl SlideState {
    pub fn unavailable() -> Self {
        SlideState { available: false, busy: false, in_show: false, current: 0, total: 0, actions: Vec::new() }
    }

    pub fn busy() -> Self {
        Self { busy: true, ..Self::unavailable() }
    }
}

/// Shared by HTTP and IPC: COM must never block the GUI or accumulate probe threads.
pub async fn read_slide_state(state: std::sync::Arc<crate::app_state::AppState>) -> SlideState {
    let permit = match state.ppt_gate.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => return SlideState::busy(),
    };
    static NEXT_PROBE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let probe_id = NEXT_PROBE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let probe_state = state.clone();
    let task = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        crate::log::with_com_context(probe_state.clone(), format!("probe-{probe_id}"), || {
            let started = std::time::Instant::now();
            crate::log::com_trace("slide_state begin");
            let result = slide_state();
            match &result {
                Ok(s) => crate::log::com_trace(format!(
                    "slide_state complete available={} busy={} in_show={} current={} total={} actions={} elapsed_ms={}",
                    s.available, s.busy, s.in_show, s.current, s.total, s.actions.len(), started.elapsed().as_millis(),
                )),
                Err(_) => crate::log::diagnostic(&probe_state, "debug", "com", format!(
                    "probe-{probe_id} slide_state failed elapsed_ms={}", started.elapsed().as_millis(),
                )),
            }
            result
        })
    });
    match tokio::time::timeout(std::time::Duration::from_secs(2), task).await {
        Ok(Ok(Ok(s))) => {
            crate::log::ppt_probe(&state, if s.available { None } else { Some("PowerPoint 未连接".into()) });
            s
        }
        result => {
            let timed_out = result.is_err();
            let error = match result {
                Ok(Ok(Err(e))) => e,
                Ok(Err(e)) => format!("读取任务异常：{e}"),
                Err(_) => "读取超时，请检查 PowerPoint 弹窗".into(),
                _ => unreachable!(),
            };
            crate::log::ppt_probe(&state, Some(error));
            if timed_out { SlideState::busy() } else { SlideState::unavailable() }
        }
    }
}

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
            let started = std::time::Instant::now();
            crate::log::com_trace("CoInitializeEx begin");
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            crate::log::com_trace(format!("CoInitializeEx complete HRESULT=0x{:08X} elapsed_ms={}", hr.0 as u32, started.elapsed().as_millis()));
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
        let started = std::time::Instant::now();
        crate::log::com_trace("connect begin");
        let result = (|| unsafe {
            let step = std::time::Instant::now();
            crate::log::com_trace("CLSIDFromProgID begin");
            let clsid = CLSIDFromProgID(w!("PowerPoint.Application"));
            trace_result("CLSIDFromProgID", step, &clsid);
            let clsid = clsid.map_err(|e| format!("找不到 PowerPoint：{e}"))?;
            let mut unk: Option<IUnknown> = None;
            let step = std::time::Instant::now();
            crate::log::com_trace("GetActiveObject begin");
            let active = GetActiveObject(&clsid, None, &mut unk);
            trace_result("GetActiveObject", step, &active);
            active.map_err(|e| format!("无法连接 PowerPoint，请确认程序已打开：{e}"))?;
            let unk = unk.ok_or_else(|| "GetActiveObject 返回空".to_string())?;
            let step = std::time::Instant::now();
            crate::log::com_trace("QueryInterface IDispatch begin");
            let dispatch = unk.cast::<IDispatch>();
            trace_result("QueryInterface IDispatch", step, &dispatch);
            dispatch.map_err(|e| e.to_string())
        })();
        crate::log::com_trace(format!("connect {} elapsed_ms={}", if result.is_ok() { "complete" } else { "failed" }, started.elapsed().as_millis()));
        result
    }

    // Never format COM results or exception descriptions: they may contain slide or VBA text.
    fn trace_result<T>(step: &str, started: std::time::Instant, result: &windows::core::Result<T>) {
        match result {
            Ok(_) => crate::log::com_trace(format!("{step} complete elapsed_ms={}", started.elapsed().as_millis())),
            Err(e) => crate::log::com_trace(format!("{step} failed HRESULT=0x{:08X} elapsed_ms={}", e.code().0 as u32, started.elapsed().as_millis())),
        }
    }

    fn invoke(disp: &IDispatch, name: &str, flags: DISPATCH_FLAGS, args: &[VARIANT]) -> Result<VARIANT, String> {
        invoke_detailed(disp, name, flags, args).map_err(|e| e.detail)
    }

    fn invoke_detailed(disp: &IDispatch, name: &str, flags: DISPATCH_FLAGS, args: &[VARIANT]) -> Result<VARIANT, crate::macro_execution::InvokeFailure> {
        let started = std::time::Instant::now();
        crate::log::com_trace(format!("member {name} begin"));
        let result = invoke_inner(disp, name, flags, args);
        match &result {
            Ok(_) => crate::log::com_trace(format!("member {name} complete elapsed_ms={}", started.elapsed().as_millis())),
            Err(e) => crate::log::com_trace(format!("member {name} failed HRESULT=0x{:08X} elapsed_ms={}", e.hresult, started.elapsed().as_millis())),
        }
        result
    }

    fn invoke_inner(disp: &IDispatch, name: &str, flags: DISPATCH_FLAGS, args: &[VARIANT]) -> Result<VARIANT, crate::macro_execution::InvokeFailure> {
        use crate::macro_execution::InvokeFailure;
        use windows::Win32::System::Com::EXCEPINFO;
        unsafe {
            let wname: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let names = [PCWSTR(wname.as_ptr())];
            let mut dispid: i32 = 0;
            let step = std::time::Instant::now();
            crate::log::com_trace(format!("GetIDsOfNames {name} begin"));
            let lookup = disp.GetIDsOfNames(&IID_NULL, names.as_ptr(), 1, LOCALE_USER_DEFAULT, &mut dispid);
            trace_result(&format!("GetIDsOfNames {name}"), step, &lookup);
            lookup.map_err(|e| InvokeFailure { hresult: e.code().0 as u32, detail: format!("{name}: 找不到成员（{e}）") })?;
            let mut argv: Vec<VARIANT> = args.iter().rev().cloned().collect();
            let params = DISPPARAMS {
                rgvarg: if argv.is_empty() { std::ptr::null_mut() } else { argv.as_mut_ptr() },
                cArgs: argv.len() as u32,
                ..Default::default()
            };
            let mut result = VARIANT::default();
            let mut exception = EXCEPINFO::default();
            let mut arg_error = u32::MAX;
            let step = std::time::Instant::now();
            crate::log::com_trace(format!("Invoke {name} flags={} begin", flags.0));
            let outcome = disp.Invoke(dispid, &IID_NULL, LOCALE_USER_DEFAULT, flags, &params,
                Some(&mut result), Some(&mut exception), Some(&mut arg_error));
            trace_result(&format!("Invoke {name}"), step, &outcome);
            if outcome.is_err() {
                if let Some(fill) = exception.pfnDeferredFillIn {
                    let step = std::time::Instant::now();
                    crate::log::com_trace(format!("DeferredFillIn {name} begin"));
                    let hr = fill(&mut exception);
                    crate::log::com_trace(format!("DeferredFillIn {name} complete HRESULT=0x{:08X} elapsed_ms={}", hr.0 as u32, step.elapsed().as_millis()));
                }
            }
            let detail = outcome.as_ref().err().map(|e| format!(
                "{name}: HRESULT=0x{:08X}; VBA code={}; scode=0x{:08X}; source={}; description={}; arg={}; {e}",
                e.code().0 as u32, exception.wCode, exception.scode as u32,
                exception.bstrSource.to_string(), exception.bstrDescription.to_string(),
                if arg_error == u32::MAX { "n/a".into() } else { arg_error.to_string() },
            ));
            // EXCEPINFO owns BSTRs, but windows-rs exposes them as ManuallyDrop.
            std::mem::ManuallyDrop::drop(&mut exception.bstrSource);
            std::mem::ManuallyDrop::drop(&mut exception.bstrDescription);
            std::mem::ManuallyDrop::drop(&mut exception.bstrHelpFile);
            if let Err(e) = outcome {
                return Err(InvokeFailure { hresult: e.code().0 as u32, detail: detail.unwrap_or_default() });
            }
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

    use windows::Win32::System::Com::DISPATCH_PROPERTYPUT;
    const DISPID_PROPERTYPUT: i32 = -3;

    /// 读字符串属性（无参 propertyget 返回 BSTR）。
    pub fn get_str(disp: &IDispatch, name: &str) -> Result<String, String> {
        call_str(disp, name, &[])
    }

    /// 写 i32 属性（PROPERTYPUT，如 View.State）。
    pub fn set_i32(disp: &IDispatch, name: &str, value: i32) -> Result<(), String> {
        unsafe {
            let wname: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let names = [PCWSTR(wname.as_ptr())];
            let mut dispid: i32 = 0;
            let step = std::time::Instant::now();
            crate::log::com_trace(format!("GetIDsOfNames {name} propertyput begin"));
            let lookup = disp.GetIDsOfNames(&IID_NULL, names.as_ptr(), 1, LOCALE_USER_DEFAULT, &mut dispid);
            trace_result(&format!("GetIDsOfNames {name} propertyput"), step, &lookup);
            lookup.map_err(|e| format!("{name}: 找不到属性（{e}）"))?;
            let mut argv = [VARIANT::from(value)];
            let mut named = [DISPID_PROPERTYPUT];
            let params = DISPPARAMS {
                rgvarg: argv.as_mut_ptr(),
                cArgs: 1,
                rgdispidNamedArgs: named.as_mut_ptr(),
                cNamedArgs: 1,
            };
            let step = std::time::Instant::now();
            crate::log::com_trace(format!("Invoke {name} propertyput begin"));
            let outcome = disp.Invoke(dispid, &IID_NULL, LOCALE_USER_DEFAULT, DISPATCH_PROPERTYPUT, &params, None, None, None);
            trace_result(&format!("Invoke {name} propertyput"), step, &outcome);
            outcome.map_err(|e| format!("{name}: 写属性失败（{e}）"))?;
            Ok(())
        }
    }

    /// 取当前页:放映视图优先,否则编辑视图。返回 (是否放映中, Slide)。
    pub fn current_slide(app: &IDispatch) -> Result<(bool, IDispatch), String> {
        let shows = get_obj(app, "SlideShowWindows")?;
        if get_i32(&shows, "Count").unwrap_or(0) >= 1 {
            let win = call_obj(&shows, "Item", &[VARIANT::from(1i32)])?;
            let view = get_obj(&win, "View")?;
            Ok((true, get_obj(&view, "Slide")?))
        } else {
            let win = get_obj(app, "ActiveWindow")?;
            let view = get_obj(&win, "View")?;
            Ok((false, get_obj(&view, "Slide")?))
        }
    }

    /// 把一个 IDispatch 包成 VT_DISPATCH 的 VARIANT(owned;由 VARIANT 的 Drop 负责 Release)。
    /// windows 0.58 已实现 `From<IDispatch> for VARIANT`(vt=9=VT_DISPATCH,转移一份所有权;
    /// clone() 先 AddRef,VARIANT 的 Drop 走 VariantClear/Release,引用计数平衡)。
    fn dispatch_variant(disp: &IDispatch) -> VARIANT {
        VARIANT::from(disp.clone())
    }

    /// Retry without the shape only if dispatch rejected the argument count.
    pub fn run_macro_with_shape(app: &IDispatch, macro_name: &str, shape: &IDispatch, report: impl FnMut(&str)) -> Result<(), String> {
        crate::macro_execution::run_with_fallback(|with_shape| {
            let mut args = vec![VARIANT::from(macro_name)];
            if with_shape { args.push(dispatch_variant(shape)); }
            invoke_detailed(app, "Run", DISPATCH_METHOD, &args).map(|_| ())
        }, report)
    }

    /// 枚举一页的形状，取所有 ppActionRunMacro 绑定 → {label, macro}。
    pub fn slide_actions(slide: &IDispatch) -> Vec<super::SlideAction> {
        let mut out = Vec::new();
        let shapes = match get_obj(slide, "Shapes") {
            Ok(s) => s,
            Err(_) => return out,
        };
        let count = get_i32(&shapes, "Count").unwrap_or(0);
        for i in 1..=count {
            let shape = match call_obj(&shapes, "Item", &[VARIANT::from(i)]) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // ActionSettings(1=ppMouseClick).Action == 8=ppActionRunMacro
            let acts = match call_obj(&shape, "ActionSettings", &[VARIANT::from(1i32)]) {
                Ok(a) => a,
                Err(_) => continue,
            };
            let action = get_i32(&acts, "Action").unwrap_or(0);
            if action == 8 {
                let macro_name = get_str(&acts, "Run").unwrap_or_default();
                if macro_name.is_empty() {
                    continue;
                }
                let id = get_str(&shape, "Name").unwrap_or_default();
                if id.is_empty() {
                    continue;
                }
                let label = shape_label(&shape);
                out.push(super::SlideAction { id, label, macro_name });
            }
        }
        out
    }

    /// 形状标签：可见文字优先，空则用形状名。
    fn shape_label(shape: &IDispatch) -> String {
        let text = get_obj(shape, "TextFrame")
            .and_then(|tf| get_obj(&tf, "TextRange"))
            .and_then(|tr| get_str(&tr, "Text"))
            .unwrap_or_default();
        let text = text.trim().to_string();
        if !text.is_empty() {
            return text;
        }
        get_str(shape, "Name").unwrap_or_default()
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

/// 读取当前页状态 + 该页的宏绑定按钮。连不上 PowerPoint 时返回 available=false（不报错）。
#[cfg(windows)]
pub fn slide_state() -> Result<SlideState, String> {
    let _co = com::ComGuard::init();
    let app = match com::connect() {
        Ok(a) => a,
        Err(_) => return Ok(SlideState::unavailable()),
    };
    let pres = com::get_obj(&app, "ActivePresentation").map_err(|_| "没有活动演示文稿".to_string())?;
    let slides = com::get_obj(&pres, "Slides")?;
    let total = com::get_i32(&slides, "Count").unwrap_or(0);

    let (in_show, slide) = com::current_slide(&app)?;
    let current = com::get_i32(&slide, "SlideIndex").unwrap_or(0);

    let actions = com::slide_actions(&slide);
    Ok(SlideState { available: true, busy: false, in_show, current, total, actions })
}

#[cfg(not(windows))]
pub fn slide_state() -> Result<SlideState, String> {
    Ok(SlideState::unavailable())
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

/// 触发当前页某个形状的鼠标动作宏(安全:仅当前页、仅 ppActionRunMacro;宏名服务端现读)。
/// 返回实际触发的宏名,供日志。
#[cfg(windows)]
pub fn trigger_action(shape_id: &str, mut report: impl FnMut(&str)) -> Result<String, String> {
    let _co = com::ComGuard::init();
    let app = com::connect()?;
    let (_in_show, slide) = com::current_slide(&app)?;
    let shapes = com::get_obj(&slide, "Shapes")?;
    // 按名定位(对增删导致的索引漂移免疫);定位不到 → 拒绝(不在当前页)。
    let shape = com::call_obj(&shapes, "Item", &[windows::core::VARIANT::from(shape_id)])
        .map_err(|e| format!("无法定位当前页形状 {shape_id}：{e}"))?;
    // ActionSettings(1=ppMouseClick).Action 必须为 8=ppActionRunMacro。
    let acts = com::call_obj(&shape, "ActionSettings", &[windows::core::VARIANT::from(1i32)])
        .map_err(|e| format!("无法读取形状鼠标动作 {shape_id}：{e}"))?;
    if com::get_i32(&acts, "Action")? != 8 {
        return Err(format!("形状不是运行宏动作:{shape_id}"));
    }
    let macro_name = com::get_str(&acts, "Run")?;
    if macro_name.is_empty() {
        return Err(format!("形状未绑定宏:{shape_id}"));
    }
    report(&format!("已解析形状 {shape_id} → 宏 {macro_name}；开始带形状调用"));
    com::run_macro_with_shape(&app, &macro_name, &shape, &mut report)
        .map_err(|e| format!("宏 {macro_name}: {e}"))?;
    Ok(macro_name)
}

/// 放映导航。
#[cfg(windows)]
fn show_view() -> Result<windows::Win32::System::Com::IDispatch, String> {
    let app = com::connect()?;
    let shows = com::get_obj(&app, "SlideShowWindows")?;
    if com::get_i32(&shows, "Count").unwrap_or(0) < 1 {
        return Err("当前没有放映".to_string());
    }
    let win = com::call_obj(&shows, "Item", &[windows::core::VARIANT::from(1i32)])?;
    com::get_obj(&win, "View")
}

#[cfg(windows)]
pub fn set_black(on: bool) -> Result<(), String> {
    let _co = com::ComGuard::init();
    // ppSlideShowBlackScreen=3, ppSlideShowRunning=1
    com::set_i32(&show_view()?, "State", if on { 3 } else { 1 })
}

#[cfg(not(windows))]
pub fn trigger_action(_shape_id: &str, _report: impl FnMut(&str)) -> Result<String, String> { Err("仅 Windows".into()) }
#[cfg(not(windows))]
pub fn set_black(_on: bool) -> Result<(), String> { Err("仅 Windows".into()) }

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
