use crate::{ErrorCode, RenderError, RenderPlan, RenderRequest};

/// Terra leaf: implement only the frozen algorithm in ../DESIGN.md.
/// Intentional red obligation: no valid request succeeds in this foundation.
pub fn render(_request: &RenderRequest) -> Result<RenderPlan, RenderError> {
    Err(RenderError {
        code: ErrorCode::RendererUnimplemented,
        field: "renderer".into(),
    })
}
