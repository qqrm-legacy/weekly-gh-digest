# deslop-please

[![slop](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2Fdeterministic-machines%2Fdeslop-please%2Fbadges%2Fslop.json)](https://github.com/deterministic-machines/deslop-please/actions/workflows/ci.yml?query=branch%3Amain)
[![test](https://github.com/deterministic-machines/deslop-please/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/deterministic-machines/deslop-please/actions/workflows/ci.yml?query=branch%3Amain)
[![crates.io](https://img.shields.io/crates/v/deslop-please.svg)](https://crates.io/crates/deslop-please)

An opinionated linter to remove sloppy code. It follows [*A Philosophy of Software Design*](https://www.amazon.com/Philosophy-Software-Design-John-Ousterhout/dp/1732102201): 
**deep** modules, **narrow** interfaces. 


```bash
cargo install deslop-please
```

```bash
> deslop-please

error[field-leak]: `RunQueue::items` is known outside `RunQueue`: 4 places in 2 modules
  --> src/sched/policy.rs:7:1
  = note: src/sched/policy.rs:7:1: `runqueue::sched::policy::admit`
  = note: src/sched/policy.rs:12:1: `runqueue::sched::policy::next`
  = note: src/sched/report.rs:4:1: `runqueue::sched::report::backlog`
  = note: src/sched/report.rs:8:1: `runqueue::sched::report::drained`
    runqueue::queue::ring::RunQueue
      items ◀━┳━ 2  runqueue::sched::policy
              ┗━ 2  runqueue::sched::report
      also known outside: head
      kept: limit

deslop-please: 65535 errors, well done!
```

### It's not my fault 

Sure, let's just mute them:

```bash
deslop-please --update-allow .deslop-please.allow  
deslop-please --allow .deslop-please.allow 
```

### Add to your `AGENTS.md`

```markdown
-  Run [`deslop-please`](https://github.com/deterministic-machines/deslop-please) to check for sloppy code structure.
-  Errors flagged by `deslop-please` must be fixed from first principles, don't do mechanical moves to hide them.
    - `module-cycle` & `tangle`: what is the indented layer of abstraction? Which module should be at which layer?
    - `field-leak` & `shallow-module`: Why we have these fields/functions at all? What are the patterns of use? Can they be hidden behind a narrow interface? Don't fix each leak one at a time, fix them holistically.
    - Draw a diagram to help user understand the before and proposed after structure.
```

## Why they are sloppy 

They are really just one single idea: **deep** modules, **narrow** interfaces.

Deep modules hide the attention for you and your agents, so you only need to care about the narrow interface.

### module-cycle

![module-cycle: before, modules kinds and socket use each other, though listen, Stream::new and BACKLOG_MOST only form a chain; after, BACKLOG_MOST moves into kinds and only socket uses kinds](docs/module-cycle.svg)


### tangle

![tangle: before, Tasks in kernel and Timers in its child module timers know each other, with no module cycle; after, tick in kernel calls both and Timers no longer knows Tasks](docs/tangle.svg)

### field-leak

![field-leak: before, functions in sched push to, index and bump the items and head of RunQueue; after, they call push, pop and backlog, and only RunQueue works on its fields](docs/field-leak.svg)

### shallow-module


![shallow-module: before, app calls 9 of the 15 functions of store; after, it calls new, load and summary, and the rest are hidden behind them](docs/shallow-module.svg)

## License

MIT
