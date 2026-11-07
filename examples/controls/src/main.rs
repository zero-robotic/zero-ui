use std::{error::Error, time::Duration};
use zui_app::Backend;
use zui_platform::WindowConfig;
use zui_types::Size;
mod demo;
fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let backend = match args.next().as_deref().unwrap_or("cpu") {
        "cpu" => Backend::Cpu,
        "gpu" => Backend::Gpu,
        _ => return Err("expected cpu or gpu".into()),
    };
    let mut smoke = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--smoke-ms" => {
                smoke = Some(Duration::from_millis(
                    args.next().ok_or("missing duration")?.parse()?,
                ))
            }
            _ => return Err(format!("unknown option: {arg}").into()),
        }
    }
    let report = demo::run(
        WindowConfig {
            title: "ZUI · 传统控件展厅".into(),
            size: Size::new(1100.0, 800.0),
            ..Default::default()
        },
        backend,
        smoke,
    )?;
    println!("events={}, redraws={}", report.events, report.redraws);
    if report.redraws == 0 {
        return Err("window never drew".into());
    }
    Ok(())
}
