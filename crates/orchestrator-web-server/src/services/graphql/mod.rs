mod mutation;
mod query;
mod subscription;
pub(crate) mod types;

use async_graphql::Schema;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse, GraphQLSubscription};
use axum::Extension;
use mutation::MutationRoot;
use orchestrator_web_api::WebApiService;
use query::QueryRoot;
use subscription::SubscriptionRoot;

pub type AoSchema = Schema<QueryRoot, MutationRoot, SubscriptionRoot>;

pub fn build_schema(api: WebApiService) -> AoSchema {
    Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .data(api)
        .finish()
}

pub fn ws_subscription(schema: AoSchema) -> GraphQLSubscription<AoSchema> {
    GraphQLSubscription::new(schema)
}

pub fn schema_sdl(schema: &AoSchema) -> String {
    schema.sdl()
}

pub async fn graphql_handler(
    Extension(schema): Extension<AoSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

pub async fn graphql_playground() -> impl axum::response::IntoResponse {
    axum::response::Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new("/graphql")
            .subscription_endpoint("/graphql/ws"),
    ))
}

pub async fn graphql_sdl_handler(
    Extension(schema): Extension<AoSchema>,
) -> impl axum::response::IntoResponse {
    schema_sdl(&schema)
}
