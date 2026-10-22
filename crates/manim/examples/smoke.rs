//! Exercise the scene protocol, the HTTP contract, and the renderer together.
//!
//! ```bash
//! docker compose up -d manim
//! cargo run -p engine-manim --example smoke
//! ```
//!
//! The render lands on the media volume, which is the point of the exercise:
//! this prints a path, not bytes, and the file is there afterwards.

use engine_manim::{
    AnimationAction, AnimationStep, Easing, Engine, ManimEngine, ManimObject, ManimRenderRequest,
    ManimScene, ManimStyle, Position, RemoteManimConfig, RemoteManimEngine, RenderOutput,
};
use engine_math::parse_expression;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = RemoteManimConfig {
        timeout: Duration::from_secs(600),
        ..RemoteManimConfig::default()
    };
    if let Ok(endpoint) = std::env::var("MANIM_ENDPOINT") {
        config.endpoint = endpoint;
    }
    config.bearer_token = std::env::var("MANIM_TOKEN").ok().filter(|t| !t.is_empty());
    let engine = RemoteManimEngine::new(config)?;

    let health = engine.health().await;
    println!(
        "health: {:?} {}",
        health.status,
        health.message.unwrap_or_default()
    );

    let at = |x: f64, y: f64| Position { x, y, z: 0.0 };
    let request = ManimRenderRequest {
        scene: ManimScene {
            title: Some("Smoke".into()),
            background_color: "background".into(),
            objects: vec![
                ManimObject::Text {
                    id: "caption".into(),
                    text: "A parabola".into(),
                    position: at(0.0, 2.5),
                    style: ManimStyle {
                        color: Some("ember".into()),
                        scale: 0.8,
                        ..ManimStyle::default()
                    },
                },
                ManimObject::Axes2d {
                    id: "axes".into(),
                    position: at(0.0, -0.5),
                    x_range: (-3.0, 3.0),
                    y_range: (-1.0, 9.0),
                    width: 6.0,
                    height: 4.0,
                    style: ManimStyle::default(),
                },
                ManimObject::Plot2d {
                    id: "curve".into(),
                    axes_id: "axes".into(),
                    expression: parse_expression("x^2")?,
                    variable: "x".into(),
                    domain: (-3.0, 3.0),
                    style: ManimStyle {
                        color: Some("current".into()),
                        stroke_width: 4.0,
                        ..ManimStyle::default()
                    },
                },
            ],
            timeline: vec![
                AnimationStep {
                    start_seconds: 0.0,
                    duration_seconds: 1.0,
                    easing: Easing::Smooth,
                    action: AnimationAction::Write {
                        object_id: "caption".into(),
                    },
                },
                AnimationStep {
                    start_seconds: 1.0,
                    duration_seconds: 1.0,
                    easing: Easing::Smooth,
                    action: AnimationAction::Create {
                        object_id: "axes".into(),
                    },
                },
                AnimationStep {
                    start_seconds: 2.0,
                    duration_seconds: 1.5,
                    easing: Easing::EaseInOut,
                    action: AnimationAction::Create {
                        object_id: "curve".into(),
                    },
                },
            ],
        },
        output: RenderOutput {
            width: 854,
            height: 480,
            ..RenderOutput::default()
        },
    };

    let started = Instant::now();
    let submission = engine.submit(request).await?;
    println!(
        "{} by {} in {:?}",
        submission.status,
        submission.renderer,
        started.elapsed()
    );

    match submission.artifact {
        Some(artifact) => println!(
            "wrote {} ({} bytes, {}x{}, {}s)",
            artifact.path,
            artifact.size_bytes,
            artifact.width,
            artifact.height,
            artifact.duration_seconds
        ),
        None => println!("the renderer named no file"),
    }
    Ok(())
}
