# Code index scope

Code-only AST index; the initial build covered 661 detected code files. Incremental updates include new code; current graph counts are in GRAPH_REPORT.md. Documentation and images were excluded by user request. No LLM/API calls were used; input/output token cost is zero. Community names are based on source paths and structural symbols, not semantic extraction.

The extractor reported syntax/partial-extraction warnings in 25 files, primarily Luau fixtures. These are parser limitations, not evidence that the game rejects those scripts. Some Luau files are represented only by file nodes.

The initial-build raw-extraction diagnostics found 1,864 dangling-endpoint relationships and 1,784 same-endpoint relationships that can collapse in an undirected graph. The builder creates unresolved symbol nodes and retains a single graph edge for some repeated relationships; the graph is useful for navigation but is not an exhaustive call/reference multigraph. See GRAPH_HEALTH.json for that initial diagnostic snapshot.
