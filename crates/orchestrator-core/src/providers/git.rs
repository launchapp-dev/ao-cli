use async_trait::async_trait;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub name: String,
    pub path: String,
    pub branch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MergeResult {
    pub merged: bool,
    pub conflicted_files: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CreatePrInput {
    pub cwd: String,
    pub base_branch: String,
    pub head_branch: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
}

#[derive(Debug, Clone)]
pub struct PullRequestInfo {
    pub id: Option<String>,
    pub number: Option<u64>,
    pub url: Option<String>,
}

#[async_trait]
pub trait GitProvider: Send + Sync {
    async fn create_worktree(
        &self,
        project_root: &str,
        worktree_path: &str,
        branch_name: &str,
        base_ref: Option<&str>,
    ) -> Result<WorktreeInfo>;

    async fn remove_worktree(&self, project_root: &str, worktree_path: &str) -> Result<()>;

    async fn push_branch(&self, cwd: &str, remote: &str, branch: &str) -> Result<()>;

    async fn is_branch_merged(&self, project_root: &str, branch_name: &str)
        -> Result<Option<bool>>;

    async fn merge_branch(
        &self,
        cwd: &str,
        source_branch: &str,
        target_branch: &str,
        no_fast_forward: bool,
    ) -> Result<MergeResult>;

    async fn create_pull_request(&self, input: CreatePrInput) -> Result<PullRequestInfo>;

    async fn enable_auto_merge(&self, cwd: &str, head_branch: &str) -> Result<()>;
}

#[derive(Default)]
pub struct GitHubProvider;

#[async_trait]
impl GitProvider for GitHubProvider {
    async fn create_worktree(
        &self,
        _project_root: &str,
        _worktree_path: &str,
        _branch_name: &str,
        _base_ref: Option<&str>,
    ) -> Result<WorktreeInfo> {
        todo!()
    }

    async fn remove_worktree(&self, _project_root: &str, _worktree_path: &str) -> Result<()> {
        todo!()
    }

    async fn push_branch(&self, _cwd: &str, _remote: &str, _branch: &str) -> Result<()> {
        todo!()
    }

    async fn is_branch_merged(
        &self,
        _project_root: &str,
        _branch_name: &str,
    ) -> Result<Option<bool>> {
        todo!()
    }

    async fn merge_branch(
        &self,
        _cwd: &str,
        _source_branch: &str,
        _target_branch: &str,
        _no_fast_forward: bool,
    ) -> Result<MergeResult> {
        todo!()
    }

    async fn create_pull_request(&self, _input: CreatePrInput) -> Result<PullRequestInfo> {
        todo!()
    }

    async fn enable_auto_merge(&self, _cwd: &str, _head_branch: &str) -> Result<()> {
        todo!()
    }
}
