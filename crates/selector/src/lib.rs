//! Explainable visualization format selection.

#![forbid(unsafe_code)]

use async_trait::async_trait;
use engine_core::{
    Engine, EngineError, EngineHealth, EngineId, EngineKind, EngineMetadata, EngineResult,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualizationIntent {
    Compare,
    Trend,
    Distribution,
    Relationship,
    Composition,
    Flow,
    Hierarchy,
    Geography,
    Schedule,
    ExplainConcept,
    DeriveEquation,
    ShowTransformation,
    TellStory,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Requirement {
    Forbidden,
    #[default]
    Optional,
    Preferred,
    Required,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Children,
    General,
    #[default]
    Student,
    Technical,
    Expert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualizationRequest {
    pub intent: VisualizationIntent,
    #[serde(default)]
    pub dimensions: u16,
    #[serde(default)]
    pub measures: u16,
    #[serde(default)]
    pub categories: u32,
    #[serde(default)]
    pub points: u32,
    #[serde(default)]
    pub temporal: bool,
    #[serde(default)]
    pub geographic: bool,
    #[serde(default)]
    pub hierarchical: bool,
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub mathematical: bool,
    #[serde(default)]
    pub spatial_3d: bool,
    #[serde(default)]
    pub narration: Requirement,
    #[serde(default)]
    pub motion: Requirement,
    #[serde(default)]
    pub interactivity: Requirement,
    pub desired_duration_seconds: Option<u32>,
    #[serde(default)]
    pub audience: Audience,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualizationFormat {
    Table,
    BarChart,
    StackedBarChart,
    LineChart,
    AreaChart,
    ScatterPlot,
    Histogram,
    BoxPlot,
    Heatmap,
    Treemap,
    Timeline,
    GanttChart,
    FlowDiagram,
    TreeDiagram,
    NetworkDiagram,
    Map,
    MathNotation,
    InteractiveGraph2d,
    InteractiveGraph3d,
    TechnicalAnimation,
    NarratedVideo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualizationTarget {
    Frontend,
    GraphingEngine,
    ManimEngine,
    RemotionEngine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualizationCandidate {
    pub format: VisualizationFormat,
    pub target: VisualizationTarget,
    pub score: u8,
    pub reasons: Vec<String>,
    pub tradeoffs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualizationSelection {
    pub selected: VisualizationCandidate,
    pub alternatives: Vec<VisualizationCandidate>,
    pub engine: String,
}

#[async_trait]
pub trait VisualizationEngine: Engine {
    async fn choose(&self, request: VisualizationRequest) -> EngineResult<VisualizationSelection>;
}

#[derive(Debug, Clone)]
pub struct RuleBasedVisualizationEngine {
    metadata: EngineMetadata,
}

impl RuleBasedVisualizationEngine {
    pub fn new() -> EngineResult<Self> {
        let metadata = EngineMetadata::new(
            EngineId::new("visualization.rules-v1")?,
            EngineKind::Visualization,
            "Explainable visualization selector",
        )
        .with_capabilities([
            "format_selection",
            "renderer_routing",
            "explainable_scoring",
            "static_visualizations",
            "interactive_graphs",
            "technical_animation",
            "narrated_video",
        ]);
        Ok(Self { metadata })
    }
}

#[async_trait]
impl Engine for RuleBasedVisualizationEngine {
    fn metadata(&self) -> EngineMetadata {
        self.metadata.clone()
    }

    async fn health(&self) -> EngineHealth {
        EngineHealth::healthy(0)
    }
}

#[derive(Default)]
struct Score {
    value: i16,
    reasons: Vec<String>,
    tradeoffs: Vec<String>,
}

#[async_trait]
impl VisualizationEngine for RuleBasedVisualizationEngine {
    async fn choose(&self, request: VisualizationRequest) -> EngineResult<VisualizationSelection> {
        validate_request(&request)?;
        let mut scores = candidates();
        score_intent(&request, &mut scores);
        score_shape(&request, &mut scores);
        score_experience(&request, &mut scores);

        let mut ranked = scores
            .into_iter()
            .map(|(format, score)| {
                (
                    score.value,
                    VisualizationCandidate {
                        format,
                        target: target_for(format),
                        score: score.value.clamp(0, 100) as u8,
                        reasons: score.reasons,
                        tradeoffs: score.tradeoffs,
                    },
                )
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| format_order(left.1.format).cmp(&format_order(right.1.format)))
        });
        let selected = ranked.remove(0).1;
        ranked.truncate(3);
        Ok(VisualizationSelection {
            selected,
            alternatives: ranked.into_iter().map(|(_, candidate)| candidate).collect(),
            engine: self.metadata.id.to_string(),
        })
    }
}

fn validate_request(request: &VisualizationRequest) -> EngineResult<()> {
    if request.dimensions > 128 || request.measures > 128 {
        return Err(EngineError::capacity(
            "visualization_fields_too_many",
            "visualization selection supports at most 128 dimensions and 128 measures",
        ));
    }
    if request.categories > 1_000_000 || request.points > 10_000_000 {
        return Err(EngineError::capacity(
            "visualization_data_shape_too_large",
            "selection metadata exceeds the supported category or point count",
        ));
    }
    if request
        .desired_duration_seconds
        .is_some_and(|value| value == 0 || value > 7_200)
    {
        return Err(EngineError::invalid_input(
            "visualization_duration_invalid",
            "desired duration must be between 1 second and 2 hours",
        ));
    }
    if request.interactivity == Requirement::Required
        && request.motion == Requirement::Required
        && request.narration == Requirement::Required
    {
        return Err(EngineError::conflict(
            "visualization_requirements_conflict",
            "a single output cannot require live interaction, animation, and continuous narration",
        ));
    }
    Ok(())
}

fn candidates() -> BTreeMap<VisualizationFormat, Score> {
    use VisualizationFormat::*;
    [
        Table,
        BarChart,
        StackedBarChart,
        LineChart,
        AreaChart,
        ScatterPlot,
        Histogram,
        BoxPlot,
        Heatmap,
        Treemap,
        Timeline,
        GanttChart,
        FlowDiagram,
        TreeDiagram,
        NetworkDiagram,
        Map,
        MathNotation,
        InteractiveGraph2d,
        InteractiveGraph3d,
        TechnicalAnimation,
        NarratedVideo,
    ]
    .into_iter()
    .map(|format| (format, Score::default()))
    .collect()
}

fn add(
    scores: &mut BTreeMap<VisualizationFormat, Score>,
    format: VisualizationFormat,
    value: i16,
    reason: &str,
) {
    let score = scores.get_mut(&format).expect("candidate exists");
    score.value += value;
    score.reasons.push(reason.into());
}

fn tradeoff(
    scores: &mut BTreeMap<VisualizationFormat, Score>,
    format: VisualizationFormat,
    value: i16,
    reason: &str,
) {
    let score = scores.get_mut(&format).expect("candidate exists");
    score.value += value;
    score.tradeoffs.push(reason.into());
}

fn score_intent(request: &VisualizationRequest, scores: &mut BTreeMap<VisualizationFormat, Score>) {
    use VisualizationFormat::*;
    match request.intent {
        VisualizationIntent::Compare => {
            add(
                scores,
                BarChart,
                65,
                "bar length supports accurate comparison",
            );
            add(scores, Table, 35, "a table preserves exact values");
        }
        VisualizationIntent::Trend => {
            add(
                scores,
                LineChart,
                70,
                "lines make change over an ordered axis legible",
            );
            add(scores, AreaChart, 45, "area emphasizes magnitude over time");
        }
        VisualizationIntent::Distribution => {
            add(
                scores,
                Histogram,
                70,
                "bins expose the shape of a distribution",
            );
            add(
                scores,
                BoxPlot,
                55,
                "quartiles make distributions compactly comparable",
            );
        }
        VisualizationIntent::Relationship => {
            add(
                scores,
                ScatterPlot,
                70,
                "position reveals relationships between measures",
            );
            add(
                scores,
                Heatmap,
                45,
                "color can summarize dense relationships",
            );
        }
        VisualizationIntent::Composition => {
            add(
                scores,
                StackedBarChart,
                65,
                "stacking shows part-to-whole change",
            );
            add(
                scores,
                Treemap,
                55,
                "area shows hierarchical part-to-whole structure",
            );
        }
        VisualizationIntent::Flow => add(
            scores,
            FlowDiagram,
            80,
            "the intent is explicitly procedural",
        ),
        VisualizationIntent::Hierarchy => add(
            scores,
            TreeDiagram,
            80,
            "the intent is explicitly hierarchical",
        ),
        VisualizationIntent::Geography => add(scores, Map, 90, "the intent is geographic"),
        VisualizationIntent::Schedule => {
            add(scores, GanttChart, 85, "a Gantt chart aligns work to time")
        }
        VisualizationIntent::ExplainConcept => {
            add(
                scores,
                TechnicalAnimation,
                65,
                "animation can reveal a concept in stages",
            );
            add(
                scores,
                FlowDiagram,
                35,
                "a static flow can summarize the explanation",
            );
        }
        VisualizationIntent::DeriveEquation => {
            add(
                scores,
                MathNotation,
                65,
                "notation is the most compact exact representation",
            );
            add(
                scores,
                TechnicalAnimation,
                55,
                "animation can reveal derivation steps sequentially",
            );
        }
        VisualizationIntent::ShowTransformation => add(
            scores,
            TechnicalAnimation,
            85,
            "motion is necessary to show how state changes",
        ),
        VisualizationIntent::TellStory => add(
            scores,
            NarratedVideo,
            90,
            "a composed timeline supports a complete narrated story",
        ),
    }
}

fn score_shape(request: &VisualizationRequest, scores: &mut BTreeMap<VisualizationFormat, Score>) {
    use VisualizationFormat::*;
    if request.geographic {
        add(scores, Map, 75, "the data carries geographic position");
    }
    if request.network {
        add(
            scores,
            NetworkDiagram,
            80,
            "the data contains nodes and relationships",
        );
    }
    if request.hierarchical {
        add(
            scores,
            TreeDiagram,
            65,
            "the data has parent-child structure",
        );
        add(
            scores,
            Treemap,
            35,
            "nested area can summarize hierarchy and magnitude",
        );
    }
    if request.temporal {
        add(
            scores,
            LineChart,
            30,
            "the data has an ordered temporal axis",
        );
        add(
            scores,
            Timeline,
            40,
            "events can be placed in chronological order",
        );
    }
    if request.mathematical {
        add(
            scores,
            InteractiveGraph2d,
            55,
            "a mathematical expression can be explored directly",
        );
        add(
            scores,
            MathNotation,
            35,
            "exact notation preserves mathematical meaning",
        );
    }
    if request.spatial_3d {
        add(
            scores,
            InteractiveGraph3d,
            80,
            "the data requires three spatial dimensions",
        );
        tradeoff(
            scores,
            InteractiveGraph2d,
            -60,
            "a 2D graph would discard a required spatial dimension",
        );
        tradeoff(scores, Table, -20, "a table hides spatial structure");
    }
    if request.points > 100_000 {
        add(
            scores,
            Heatmap,
            25,
            "aggregation avoids drawing every point",
        );
        tradeoff(
            scores,
            ScatterPlot,
            -30,
            "a raw scatter plot would overdraw heavily",
        );
    }
    if request.categories > 30 {
        add(
            scores,
            Table,
            20,
            "many categories remain searchable in a table",
        );
        tradeoff(
            scores,
            BarChart,
            -25,
            "too many bars become difficult to scan",
        );
    }
}

fn score_experience(
    request: &VisualizationRequest,
    scores: &mut BTreeMap<VisualizationFormat, Score>,
) {
    use VisualizationFormat::*;
    match request.narration {
        Requirement::Required => add(scores, NarratedVideo, 70, "narration is required"),
        Requirement::Preferred => add(scores, NarratedVideo, 35, "narration is preferred"),
        Requirement::Forbidden => tradeoff(scores, NarratedVideo, -100, "narration is forbidden"),
        Requirement::Optional => {}
    }
    match request.motion {
        Requirement::Required => add(scores, TechnicalAnimation, 60, "motion is required"),
        Requirement::Preferred => add(scores, TechnicalAnimation, 30, "motion is preferred"),
        Requirement::Forbidden => {
            tradeoff(scores, TechnicalAnimation, -100, "motion is forbidden");
            tradeoff(
                scores,
                NarratedVideo,
                -40,
                "video necessarily introduces a timeline",
            );
        }
        Requirement::Optional => {}
    }
    match request.interactivity {
        Requirement::Required => {
            add(
                scores,
                InteractiveGraph2d,
                50,
                "live interaction is required",
            );
            add(
                scores,
                InteractiveGraph3d,
                50,
                "live interaction is required",
            );
            tradeoff(
                scores,
                TechnicalAnimation,
                -60,
                "rendered animation is not directly interactive",
            );
            tradeoff(
                scores,
                NarratedVideo,
                -60,
                "rendered video is not directly interactive",
            );
        }
        Requirement::Preferred => {
            add(
                scores,
                InteractiveGraph2d,
                20,
                "live exploration is preferred",
            );
            add(
                scores,
                InteractiveGraph3d,
                20,
                "live exploration is preferred",
            );
        }
        Requirement::Forbidden => {
            tradeoff(
                scores,
                InteractiveGraph2d,
                -100,
                "interactivity is forbidden",
            );
            tradeoff(
                scores,
                InteractiveGraph3d,
                -100,
                "interactivity is forbidden",
            );
        }
        Requirement::Optional => {}
    }
    if request
        .desired_duration_seconds
        .is_some_and(|duration| duration >= 90)
    {
        add(
            scores,
            NarratedVideo,
            35,
            "the requested duration suits a composed explainer",
        );
        tradeoff(
            scores,
            TechnicalAnimation,
            -10,
            "long-form pacing needs composition and narration tools",
        );
    }
    if matches!(request.audience, Audience::Children | Audience::General)
        && request.intent == VisualizationIntent::ExplainConcept
    {
        add(
            scores,
            TechnicalAnimation,
            15,
            "staged visuals reduce abstraction for this audience",
        );
    }
}

fn target_for(format: VisualizationFormat) -> VisualizationTarget {
    use VisualizationFormat::*;
    match format {
        InteractiveGraph2d | InteractiveGraph3d => VisualizationTarget::GraphingEngine,
        TechnicalAnimation => VisualizationTarget::ManimEngine,
        NarratedVideo => VisualizationTarget::RemotionEngine,
        _ => VisualizationTarget::Frontend,
    }
}

fn format_order(format: VisualizationFormat) -> u8 {
    use VisualizationFormat::*;
    match format {
        Table => 0,
        BarChart => 1,
        StackedBarChart => 2,
        LineChart => 3,
        AreaChart => 4,
        ScatterPlot => 5,
        Histogram => 6,
        BoxPlot => 7,
        Heatmap => 8,
        Treemap => 9,
        Timeline => 10,
        GanttChart => 11,
        FlowDiagram => 12,
        TreeDiagram => 13,
        NetworkDiagram => 14,
        Map => 15,
        MathNotation => 16,
        InteractiveGraph2d => 17,
        InteractiveGraph3d => 18,
        TechnicalAnimation => 19,
        NarratedVideo => 20,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(intent: VisualizationIntent) -> VisualizationRequest {
        VisualizationRequest {
            intent,
            dimensions: 1,
            measures: 1,
            categories: 8,
            points: 100,
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

    #[tokio::test]
    async fn selects_a_line_chart_for_a_temporal_trend() {
        let engine = RuleBasedVisualizationEngine::new().unwrap();
        let mut input = request(VisualizationIntent::Trend);
        input.temporal = true;
        let result = engine.choose(input).await.unwrap();
        assert_eq!(result.selected.format, VisualizationFormat::LineChart);
        assert!(!result.selected.reasons.is_empty());
    }

    #[tokio::test]
    async fn documented_minimal_tool_input_deserializes() {
        let input: VisualizationRequest = serde_json::from_value(serde_json::json!({
            "intent": "trend",
            "temporal": true,
            "points": 365
        }))
        .unwrap();
        let result = RuleBasedVisualizationEngine::new()
            .unwrap()
            .choose(input)
            .await
            .unwrap();
        assert_eq!(result.selected.format, VisualizationFormat::LineChart);
    }

    #[tokio::test]
    async fn routes_a_required_narrated_story_to_remotion() {
        let engine = RuleBasedVisualizationEngine::new().unwrap();
        let mut input = request(VisualizationIntent::TellStory);
        input.narration = Requirement::Required;
        input.desired_duration_seconds = Some(180);
        let result = engine.choose(input).await.unwrap();
        assert_eq!(result.selected.format, VisualizationFormat::NarratedVideo);
        assert_eq!(result.selected.target, VisualizationTarget::RemotionEngine);
    }

    #[tokio::test]
    async fn selects_3d_graphing_for_spatial_mathematics() {
        let engine = RuleBasedVisualizationEngine::new().unwrap();
        let mut input = request(VisualizationIntent::Relationship);
        input.mathematical = true;
        input.spatial_3d = true;
        input.interactivity = Requirement::Required;
        let result = engine.choose(input).await.unwrap();
        assert_eq!(
            result.selected.format,
            VisualizationFormat::InteractiveGraph3d
        );
        assert_eq!(result.selected.target, VisualizationTarget::GraphingEngine);
    }

    #[tokio::test]
    async fn rejects_impossible_experience_requirements() {
        let engine = RuleBasedVisualizationEngine::new().unwrap();
        let mut input = request(VisualizationIntent::TellStory);
        input.narration = Requirement::Required;
        input.motion = Requirement::Required;
        input.interactivity = Requirement::Required;
        let error = engine.choose(input).await.unwrap_err();
        assert_eq!(error.code, "visualization_requirements_conflict");
    }
}
