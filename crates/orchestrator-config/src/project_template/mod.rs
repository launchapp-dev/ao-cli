pub mod loading;
pub mod types;

#[cfg(test)]
mod tests;

pub use loading::{
    default_project_template_registry_url, list_project_templates_from_default_registry,
    list_project_templates_from_registry_root, load_project_template_from_default_registry,
    load_project_template_from_dir, load_project_template_from_file, load_project_template_from_registry_root,
    parse_project_template_manifest, sync_default_project_template_registry, PROJECT_TEMPLATE_REGISTRY_URL_ENV,
};
pub use types::*;
