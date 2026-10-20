# TODO: MCT Documentation

**Status:** Static mdBook product documentation and rustdoc API builds are
implemented. The unused conversation-archive pilot has been retired; its
original plan and implementation remain in Git at `f315175`.

## Boundaries

- Authored Markdown and Rust source remain usable without presentation tooling.
- mdBook renders product documentation under `/docs/`; rustdoc renders APIs
  under `/api/`.
- `layer/` retains specifications, evidence, beliefs, and written session history.
- Documentation builds check links and reject rustdoc warnings.
- Explanations cite supporting decisions, evidence, and commits. They do not
  turn summaries into implementation authority or proof of owner approval.

## Remaining documentation work

- [ ] Complete introduction and product-evaluation guidance.
- [ ] Complete getting-started tutorials and task-oriented how-to guides.
- [ ] Explain Mother/Child/Toy concepts and architecture.
- [ ] Complete operations and troubleshooting guidance.
- [ ] Complete Child-development and integration guides.
- [ ] Generate CLI, configuration, protocol, and schema references from
  implementation sources where practical.
- [ ] Detect stale generated references.
- [ ] Add external-link checking when deployment policy is set.

Keep generated references reproducible and reviewed explanations as ordinary
Markdown. No historical conversation import, transcript renderer, or automatic
session-publication workflow is planned.
