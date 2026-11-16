use nacos_macro::response;

use crate::ai::model::prompt::Prompt;

#[response(identity = "QueryPromptResponse", module = "ai")]
pub(crate) struct QueryPromptResponse {
    pub(crate) prompt_info: Option<Prompt>,
}
