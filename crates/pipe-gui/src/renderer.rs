//! Window renderer choice: wgpu first (DX12/Vulkan/Metal/GL), glow as fallback.

use crate::app::{GuiApp, LaunchCfg};
use crate::report_error;

/// How `pipe-gui` should pick an `eframe` backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GuiRenderer {
    /// Try wgpu, then glow if wgpu cannot start.
    #[default]
    Auto,
    /// Direct3D 12 / Vulkan / Metal / GL via wgpu. No glow fallback.
    Wgpu,
    /// OpenGL 2.0 via glow. No wgpu attempt.
    Glow,
}

pub const ENV_RENDERER: &str = "PIPE_GUI_RENDERER";

impl GuiRenderer {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "wgpu" => Ok(Self::Wgpu),
            "glow" => Ok(Self::Glow),
            other => Err(format!(
                "unknown renderer {other:?}; use auto, wgpu, or glow"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Wgpu => "wgpu",
            Self::Glow => "glow",
        }
    }

    /// Backends to try, in order. Auto is wgpu then glow.
    pub fn backends(self) -> &'static [eframe::Renderer] {
        match self {
            Self::Auto => &[eframe::Renderer::Wgpu, eframe::Renderer::Glow],
            Self::Wgpu => &[eframe::Renderer::Wgpu],
            Self::Glow => &[eframe::Renderer::Glow],
        }
    }
}

pub fn from_env() -> Result<Option<GuiRenderer>, String> {
    match std::env::var(ENV_RENDERER) {
        Ok(s) if !s.trim().is_empty() => GuiRenderer::parse(&s).map(Some),
        Ok(_) | Err(std::env::VarError::NotPresent) => Ok(None),
        Err(e) => Err(format!("{ENV_RENDERER}: {e}")),
    }
}

/// CLI `--renderer` wins over `PIPE_GUI_RENDERER`. Unset means Auto.
pub fn resolve(cli: Option<GuiRenderer>) -> Result<GuiRenderer, String> {
    if let Some(choice) = cli {
        return Ok(choice);
    }
    Ok(from_env()?.unwrap_or(GuiRenderer::Auto))
}

pub fn backend_plain_name(backend: eframe::Renderer) -> &'static str {
    match backend {
        eframe::Renderer::Wgpu => "Direct3D / Vulkan / Metal (wgpu)",
        eframe::Renderer::Glow => "OpenGL 2.0 (glow)",
    }
}

pub fn native_options(renderer: eframe::Renderer) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("Portable Pipelines"),
        renderer,
        vsync: true,
        centered: true,
        ..Default::default()
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "the graphics library aborted during startup".into()
    }
}

fn try_run(renderer: eframe::Renderer, cfg: LaunchCfg) -> Result<(), String> {
    // wgpu 25 panics (does not return Err) when no backend feature is compiled
    // in, and some driver failures abort the same way. Catch that so Auto can
    // still try glow.
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        eframe::run_native(
            "Portable Pipelines",
            native_options(renderer),
            Box::new(move |cc| Ok(Box::new(GuiApp::new(cc, cfg)))),
        )
    })) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(payload) => Err(panic_message(payload)),
    }
}

pub fn fallback_notice(failed: eframe::Renderer, err: &str, next: eframe::Renderer) -> String {
    format!(
        "pipe-gui: {} could not start:\n  {err}\n\
         Trying {} next. To skip this, use --renderer {} or {ENV_RENDERER}={}.",
        backend_plain_name(failed),
        backend_plain_name(next),
        match next {
            eframe::Renderer::Wgpu => "wgpu",
            eframe::Renderer::Glow => "glow",
        },
        match next {
            eframe::Renderer::Wgpu => "wgpu",
            eframe::Renderer::Glow => "glow",
        },
    )
}

pub fn both_failed(choice: GuiRenderer, errors: &[(eframe::Renderer, String)]) -> String {
    let mut lines = Vec::new();
    lines.push(
        "pipe-gui could not open a window. This computer did not provide a working graphics API."
            .into(),
    );
    lines.push(String::new());
    for (backend, err) in errors {
        lines.push(format!("{}:\n  {err}", backend_plain_name(*backend)));
        lines.push(String::new());
    }
    match choice {
        GuiRenderer::Auto => lines.push(
            "On some Windows laptops OpenGL is missing when you sit at the machine, even when \
Device Manager shows a healthy NVIDIA adapter and Microsoft Basic Display Adapter, while the \
same build works over Remote Desktop. The default is to try Direct3D/Vulkan first, then OpenGL."
                .into(),
        ),
        GuiRenderer::Wgpu => lines.push(
            "This --renderer wgpu override does not fall back to OpenGL. Try --renderer auto \
(the default) or --renderer glow."
                .into(),
        ),
        GuiRenderer::Glow => lines.push(
            "This --renderer glow override does not try Direct3D/Vulkan. Try --renderer auto \
(the default) or --renderer wgpu."
                .into(),
        ),
    }
    lines.push(String::new());
    lines.push("Force one backend:".into());
    lines.push("  pipe --gui --renderer auto".into());
    lines.push("  pipe --gui --renderer wgpu".into());
    lines.push("  pipe --gui --renderer glow".into());
    lines.push(format!("  {ENV_RENDERER}=wgpu   (or glow, or auto)"));
    lines.join("\n")
}

/// Open the native window. Auto tries wgpu, then glow. Forced backends do not fall back.
pub fn run_gui(cfg: LaunchCfg, choice: GuiRenderer) -> Result<(), String> {
    let backends = choice.backends();
    let mut errors: Vec<(eframe::Renderer, String)> = Vec::new();
    for (i, backend) in backends.iter().copied().enumerate() {
        eprintln!("pipe-gui: starting with {}", backend_plain_name(backend));
        match try_run(backend, cfg.clone()) {
            Ok(()) => return Ok(()),
            Err(msg) => {
                if let Some(next) = backends.get(i + 1).copied() {
                    report_error(&fallback_notice(backend, &msg, next));
                }
                errors.push((backend, msg));
            }
        }
    }
    Err(both_failed(choice, &errors))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_names() {
        assert_eq!(GuiRenderer::parse("auto").unwrap(), GuiRenderer::Auto);
        assert_eq!(GuiRenderer::parse("WGPU").unwrap(), GuiRenderer::Wgpu);
        assert_eq!(GuiRenderer::parse(" glow ").unwrap(), GuiRenderer::Glow);
        let err = GuiRenderer::parse("opengl").unwrap_err();
        assert!(err.contains("auto, wgpu, or glow"), "{err}");
    }

    #[test]
    fn auto_tries_wgpu_then_glow() {
        assert_eq!(
            GuiRenderer::Auto.backends(),
            &[eframe::Renderer::Wgpu, eframe::Renderer::Glow]
        );
        assert_eq!(GuiRenderer::Wgpu.backends(), &[eframe::Renderer::Wgpu]);
        assert_eq!(GuiRenderer::Glow.backends(), &[eframe::Renderer::Glow]);
    }

    #[test]
    fn resolve_prefers_cli() {
        assert_eq!(resolve(Some(GuiRenderer::Glow)).unwrap(), GuiRenderer::Glow);
    }

    #[test]
    fn failure_text_names_cause_and_override() {
        let msg = both_failed(
            GuiRenderer::Auto,
            &[
                (eframe::Renderer::Wgpu, "no adapter".into()),
                (eframe::Renderer::Glow, "OpenGL 2.0 is not available".into()),
            ],
        );
        assert!(msg.contains("Direct3D"), "{msg}");
        assert!(msg.contains("OpenGL 2.0"), "{msg}");
        assert!(msg.contains("--renderer glow"), "{msg}");
        assert!(msg.contains(ENV_RENDERER), "{msg}");
        assert!(msg.contains("Remote Desktop"), "{msg}");
        assert!(msg.contains("no adapter"), "{msg}");
    }

    #[test]
    fn wgpu_only_failure_does_not_claim_fallback() {
        let msg = both_failed(
            GuiRenderer::Wgpu,
            &[(eframe::Renderer::Wgpu, "dx12 missing".into())],
        );
        assert!(msg.contains("does not fall back to OpenGL"), "{msg}");
        assert!(!msg.contains("Microsoft Basic Display"), "{msg}");
    }

    #[test]
    fn panic_message_keeps_wgpu_text() {
        let msg = panic_message(Box::new(
            "No wgpu backend feature that is implemented for the target platform was enabled.",
        ));
        assert!(msg.contains("No wgpu backend feature"), "{msg}");
    }

    #[test]
    fn fallback_notice_is_plain() {
        let n = fallback_notice(
            eframe::Renderer::Wgpu,
            "adapter init failed",
            eframe::Renderer::Glow,
        );
        assert!(n.contains("Trying OpenGL 2.0 (glow) next"), "{n}");
        assert!(n.contains("--renderer glow"), "{n}");
    }
}
