//! Turn a plain-text function into a validated, renderer-neutral graph spec.
//!
//! ```bash
//! cargo run -p engine-graphing --example plot_spec -- "sin(x) / x"
//! cargo run -p engine-graphing --example plot_spec -- "x + secret"   # refused
//! ```

use engine_graphing::{
    GraphInteraction, GraphRequest, GraphSeries, GraphingEngine, LineStyle, Range,
    SpecificationGraphingEngine,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let expression = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "x^2 - 5x + 6".into());
    let engine = SpecificationGraphingEngine::new()?;
    let result = engine
        .generate(GraphRequest {
            title: Some(format!("y = {expression}")),
            series: vec![GraphSeries::Function2d {
                id: "curve".into(),
                label: Some(expression.clone()),
                expression,
                variable: "x".into(),
                domain: Range::new(-10.0, 10.0),
                style: LineStyle::default(),
            }],
            axes: None,
            viewport: None,
            interaction: GraphInteraction::default(),
        })
        .await;
    match result {
        Ok(spec) => println!("{}", serde_json::to_string_pretty(&spec)?),
        Err(error) => {
            eprintln!("refused: {error}");
            std::process::exit(1);
        }
    }
    Ok(())
}
