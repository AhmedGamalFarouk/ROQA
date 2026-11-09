use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
        UI::Accessibility::*,
    },
    core::Result,
};
fn json(value: &str) -> String {
    let mut output = String::from("\"");
    for unit in value.encode_utf16() {
        match unit {
            34 => output.push_str("\\\""),
            92 => output.push_str("\\\\"),
            32..=126 => output.push(char::from_u32(unit as u32).unwrap()),
            _ => output.push_str(&format!("\\u{unit:04X}")),
        }
    }
    output.push('"');
    output
}
fn run() -> Result<()> {
    // SAFETY: standalone synchronous client; balanced after all automation objects have been released.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
    }
    let result = inspect();
    // SAFETY: inspect's COM locals have been dropped on this same thread.
    unsafe {
        CoUninitialize();
    }
    result
}
fn inspect() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let command = args.first().expect("Expected command");
    let handle = args
        .get(1)
        .expect("Expected HWND")
        .parse::<usize>()
        .expect("Expected numeric HWND");
    // SAFETY: creates a standard out-of-process automation client; HWND is an opaque scalar supplied by owned test.
    unsafe {
        let client: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let element = client.ElementFromHandle(HWND(handle as *mut _))?;
        match command.as_str() {
            "list" => {
                let children =
                    element.FindAll(TreeScope_Children, &client.CreateTrueCondition()?)?;
                let mut names = Vec::new();
                for index in 0..children.Length()? {
                    let item = children.GetElement(index)?;
                    if item.CurrentControlType()? == UIA_ListItemControlTypeId {
                        names.push(json(&item.CurrentName()?.to_string()));
                    }
                }
                println!("[{}]", names.join(","));
            }
            "buttons" | "invoke" => {
                let children =
                    element.FindAll(TreeScope_Children, &client.CreateTrueCondition()?)?;
                let mut buttons = Vec::new();
                for index in 0..children.Length()? {
                    let item = children.GetElement(index)?;
                    if item.CurrentControlType()? == UIA_ButtonControlTypeId {
                        buttons.push(item);
                    }
                }
                if command == "invoke" {
                    let slot: usize = args
                        .get(2)
                        .expect("Expected slot")
                        .parse()
                        .expect("Numeric slot");
                    let invoke: IUIAutomationInvokePattern =
                        buttons[slot].GetCurrentPatternAs(UIA_InvokePatternId)?;
                    invoke.Invoke()?;
                    println!("true");
                } else {
                    let mut rows = Vec::new();
                    for item in &buttons {
                        let rect = item.CurrentBoundingRectangle()?;
                        let invokable = item.GetCurrentPattern(UIA_InvokePatternId).is_ok();
                        rows.push(format!(
                            "{{\"name\":{},\"invoke\":{},\"rect\":[{},{},{},{}]}}",
                            json(&item.CurrentName()?.to_string()),
                            invokable,
                            rect.left,
                            rect.top,
                            rect.right - rect.left,
                            rect.bottom - rect.top
                        ));
                    }
                    println!("[{}]", rows.join(","));
                }
            }
            "value" => {
                let value: IUIAutomationValuePattern =
                    element.GetCurrentPatternAs(UIA_ValuePatternId)?;
                println!("{}", json(&value.CurrentValue()?.to_string()));
            }
            "set" => {
                let value: IUIAutomationValuePattern =
                    element.GetCurrentPatternAs(UIA_ValuePatternId)?;
                let text = windows::core::BSTR::from(args.get(2).expect("Expected value").as_str());
                value.SetValue(&text)?;
                println!("true");
            }
            "inspect" => {
                println!(
                    "{{\"name\":{},\"role\":{},\"focusable\":{},\"enabled\":{},\"class\":{}}}",
                    json(&element.CurrentName()?.to_string()),
                    element.CurrentControlType()?.0,
                    element.CurrentIsKeyboardFocusable()?.as_bool(),
                    element.CurrentIsEnabled()?.as_bool(),
                    json(&element.CurrentClassName()?.to_string())
                );
            }
            _ => panic!("Unknown command"),
        }
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("UIA probe: {error}");
        std::process::exit(1);
    }
}
