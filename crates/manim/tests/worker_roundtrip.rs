//! A render through a running Manim worker.
//!
//! Skipped unless `WORKER_TESTS=1`. To run it:
//!
//! ```bash
//! docker compose up -d manim
//! WORKER_TESTS=1 cargo test -p engine-manim --test worker_roundtrip
//! ```
//!
//! `MANIM_ENDPOINT` and `MANIM_TOKEN` are honoured.

use engine_manim::{
    AnimationAction, AnimationStep, Easing, ManimEngine, ManimObject, ManimRenderRequest,
    ManimScene, ManimStyle, Position, RemoteManimConfig, RemoteManimEngine, RenderOutput,
};
use engine_math::parse_equation;
use std::time::Duration;

fn enabled() -> bool {
    if std::env::var("WORKER_TESTS").as_deref() == Ok("1") {
        return true;
    }
    eprintln!("skipped: set WORKER_TESTS=1 with the manim worker running");
    false
}

fn engine() -> RemoteManimEngine {
    let mut config = RemoteManimConfig {
        timeout: Duration::from_secs(300),
        ..RemoteManimConfig::default()
    };
    if let Ok(endpoint) = std::env::var("MANIM_ENDPOINT") {
        config.endpoint = endpoint;
    }
    config.bearer_token = std::env::var("MANIM_TOKEN").ok().filter(|t| !t.is_empty());
    RemoteManimEngine::new(config).unwrap()
}

#[tokio::test]
async fn renders_a_typeset_equation_to_a_file() {
    if !enabled() {
        return;
    }
    let request = ManimRenderRequest {
        scene: ManimScene {
            title: None,
            background_color: "background".into(),
            objects: vec![ManimObject::Equation {
                id: "eq".into(),
                equation: parse_equation("x^2 = 4").unwrap(),
                position: Position {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                style: ManimStyle::default(),
            }],
            timeline: vec![AnimationStep {
                start_seconds: 0.0,
                duration_seconds: 0.5,
                easing: Easing::Smooth,
                action: AnimationAction::Write {
                    object_id: "eq".into(),
                },
            }],
        },
        output: RenderOutput {
            width: 320,
            height: 240,
            frames_per_second: 12,
            format: "mp4".into(),
            transparent: false,
        },
    };
    let submission = engine().submit(request).await.unwrap();
    assert_eq!(submission.status, "completed");
    let artifact = submission
        .artifact
        .expect("a completed render names its file");
    assert!(artifact.path.starts_with("renders/"));
    assert!(artifact.path.ends_with(".mp4"));
    assert!(artifact.size_bytes > 0);
}
