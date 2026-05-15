pub(crate) mod infer;
pub(crate) mod infer_bct;
pub(crate) mod infer_candidate_kinds;
pub(crate) mod infer_matching;
pub(crate) mod infer_resolve;
mod template_segment_prefix;

pub(crate) use infer::InferenceContext;
