# Algraf v0.39.0 Plan

Status: Superseded by [`V0_40_PLAN.md`](V0_40_PLAN.md)
Owner: Algraf maintainers
Related spec: [`ALGRAF_SPEC.md`](ALGRAF_SPEC.md)
Predecessor plan: [`V0_38_PLAN.md`](V0_38_PLAN.md)
Follow-on plan: [`V0_39_5_PLAN.md`](V0_39_5_PLAN.md)
Roadmap theme: ggplot2 feature comparability without ggplot2 API compatibility.

## Superseded Scope

The model and summary-stat work originally planned for v0.39.0 has been merged
into [`V0_40_PLAN.md`](V0_40_PLAN.md). No independent v0.39.0 release is planned.

The merged scope includes:

- computed-stat variable policy;
- identity/distinct, ECDF, QQ, summary, and summary-bin stats;
- evaluation of quantile regression;
- shared numeric helper and stat performance follow-ups;
- examples, README updates, LSP metadata, and equivalence tests for those stats.

The reason for merging is product coherence: Algraf should not ask users to
prepare summary, ECDF, QQ, or binned-class data outside the tool. v0.40 now owns
the full path from ordinary input data through `Derive` stats to scale and guide
control.

See [`V0_40_PLAN.md`](V0_40_PLAN.md) for the active release plan.
