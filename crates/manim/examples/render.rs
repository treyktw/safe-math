//! Render a short "solve, then plot" scene through the Manim worker.
//!
//! ```bash
//! docker compose up -d manim
//! cargo run -p engine-manim --example render -- gif
//! docker compose cp manim:/media/<path printed above> docs/quadratic.gif
//! ```
//!
//! Every mathematical object in the scene is a parsed AST from the math
//! crate. The worker turns it into LaTeX itself; no LaTeX string and no Python
//! is ever sent, so a scene cannot smuggle `\input{/etc/passwd}` or code.

use engine_manim::{
    AnimationAction, AnimationStep, Easing, Engine, ManimEngine, ManimObject, ManimRenderRequest,
    ManimScene, ManimStyle, Position, RemoteManimConfig, RemoteManimEngine, RenderOutput,
    validate_request,
};
use engine_math::{parse_equation, parse_expression};
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let format = std::env::args().nth(1).unwrap_or_else(|| "mp4".into());

    let mut config = RemoteManimConfig {
        timeout: Duration::from_secs(600),
        ..RemoteManimConfig::default()
    };
    if let Ok(endpoint) = std::env::var("MANIM_ENDPOINT") {
        config.endpoint = endpoint;
    }
    config.bearer_token = std::env::var("MANIM_TOKEN").ok().filter(|t| !t.is_empty());
    let engine = RemoteManimEngine::new(config)?;

    let at = |x: f64, y: f64| Position { x, y, z: 0.0 };
    let step = |start: f64, duration: f64, action: AnimationAction| AnimationStep {
        start_seconds: start,
        duration_seconds: duration,
        easing: Easing::Smooth,
        action,
    };
    let equation_style = ManimStyle {
        color: Some("foreground".into()),
        ..ManimStyle::default()
    };

    let request = ManimRenderRequest {
        scene: ManimScene {
            title: Some("Solve, then plot".into()),
            background_color: "background".into(),
            objects: vec![
                ManimObject::Equation {
                    id: "standard-form".into(),
                    equation: parse_equation("x^2 - 5x + 6 = 0")?,
                    position: at(0.0, 3.0),
                    style: equation_style.clone(),
                },
                ManimObject::Equation {
                    id: "factored-form".into(),
                    equation: parse_equation("(x - 2)(x - 3) = 0")?,
                    position: at(0.0, 3.0),
                    style: equation_style,
                },
                ManimObject::Axes2d {
                    id: "axes".into(),
                    position: at(0.0, -0.4),
                    x_range: (0.0, 5.0),
                    y_range: (-1.0, 6.0),
                    width: 7.0,
                    height: 4.2,
                    style: ManimStyle {
                        color: Some("muted".into()),
                        ..ManimStyle::default()
                    },
                },
                ManimObject::Plot2d {
                    id: "parabola".into(),
                    axes_id: "axes".into(),
                    expression: parse_expression("x^2 - 5x + 6")?,
                    variable: "x".into(),
                    domain: (0.2, 4.8),
                    style: ManimStyle {
                        color: Some("aurora".into()),
                        stroke_width: 5.0,
                        ..ManimStyle::default()
                    },
                },
                ManimObject::Text {
                    id: "roots".into(),
                    text: "roots at x = 2 and x = 3".into(),
                    position: at(0.0, -3.3),
                    style: ManimStyle {
                        color: Some("success".into()),
                        scale: 0.6,
                        ..ManimStyle::default()
                    },
                },
            ],
            timeline: vec![
                step(
                    0.0,
                    1.5,
                    AnimationAction::Write {
                        object_id: "standard-form".into(),
                    },
                ),
                step(
                    2.0,
                    1.2,
                    AnimationAction::Transform {
                        from_id: "standard-form".into(),
                        to_id: "factored-form".into(),
                    },
                ),
                step(
                    3.5,
                    1.0,
                    AnimationAction::Create {
                        object_id: "axes".into(),
                    },
                ),
                step(
                    4.5,
                    1.5,
                    AnimationAction::Create {
                        object_id: "parabola".into(),
                    },
                ),
                step(
                    6.2,
                    0.8,
                    AnimationAction::FadeIn {
                        object_id: "roots".into(),
                    },
                ),
                step(7.0, 1.0, AnimationAction::Wait),
            ],
        },
        output: RenderOutput {
            width: 640,
            height: 360,
            frames_per_second: if format == "gif" { 15 } else { 30 },
            format,
            transparent: false,
        },
    };

    // The client validates again inside `submit`; this just fails early with
    // a readable error before anything is sent.
    validate_request(&request)?;

    let health = engine.health().await;
    println!("renderer health: {:?}", health.status);

    let started = Instant::now();
    let submission = engine.submit(request).await?;
    println!(
        "{} by {} in {:.1?}",
        submission.status,
        submission.renderer,
        started.elapsed()
    );
    match submission.artifact {
        Some(artifact) => {
            println!(
                "wrote {} ({} bytes, {}x{} @ {} fps, {}s)",
                artifact.path,
                artifact.size_bytes,
                artifact.width,
                artifact.height,
                artifact.frames_per_second,
                artifact.duration_seconds
            );
            println!(
                "copy it out with:\n  docker compose cp manim:/media/{} ./scene.{}",
                artifact.path, artifact.format
            );
        }
        None => println!("the renderer named no file"),
    }
    Ok(())
}
