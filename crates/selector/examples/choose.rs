//! Ask the selector how to show something, and print why.
//!
//! ```bash
//! cargo run -p engine-selector --example choose
//! ```

use engine_selector::{
    Audience, Requirement, RuleBasedVisualizationEngine, VisualizationEngine, VisualizationIntent,
    VisualizationRequest,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = RuleBasedVisualizationEngine::new()?;
    let scenarios = [
        (
            "derive the quadratic formula for a student",
            VisualizationRequest {
                intent: VisualizationIntent::DeriveEquation,
                mathematical: true,
                motion: Requirement::Preferred,
                ..base(VisualizationIntent::DeriveEquation)
            },
        ),
        (
            "explore z = sin(x) cos(y) interactively",
            VisualizationRequest {
                mathematical: true,
                spatial_3d: true,
                interactivity: Requirement::Required,
                ..base(VisualizationIntent::Relationship)
            },
        ),
        (
            "daily signups over a year",
            VisualizationRequest {
                temporal: true,
                points: 365,
                ..base(VisualizationIntent::Trend)
            },
        ),
    ];
    for (question, request) in scenarios {
        let selection = engine.choose(request).await?;
        println!("\n{question}");
        println!(
            "  -> {:?} via {:?} (score {})",
            selection.selected.format, selection.selected.target, selection.selected.score
        );
        for reason in &selection.selected.reasons {
            println!("     + {reason}");
        }
        for tradeoff in &selection.selected.tradeoffs {
            println!("     - {tradeoff}");
        }
        let alternatives = selection
            .alternatives
            .iter()
            .map(|candidate| format!("{:?} ({})", candidate.format, candidate.score))
            .collect::<Vec<_>>()
            .join(", ");
        println!("     alternatives: {alternatives}");
    }
    Ok(())
}

fn base(intent: VisualizationIntent) -> VisualizationRequest {
    VisualizationRequest {
        intent,
        dimensions: 0,
        measures: 0,
        categories: 0,
        points: 0,
        temporal: false,
        geographic: false,
        hierarchical: false,
        network: false,
        mathematical: false,
        spatial_3d: false,
        narration: Requirement::Optional,
        motion: Requirement::Optional,
        interactivity: Requirement::Optional,
        desired_duration_seconds: None,
        audience: Audience::Student,
    }
}
