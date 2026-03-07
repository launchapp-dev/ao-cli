use anyhow::Result;
use async_trait::async_trait;

use crate::{ProjectTickAction, ProjectTickActionEffect};

#[async_trait(?Send)]
pub trait ProjectTickActionExecutor {
    async fn execute_action(
        &mut self,
        action: &ProjectTickAction,
    ) -> Result<ProjectTickActionEffect>;
}
