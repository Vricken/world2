## Game Coding Standard

### Core Philosophy

Build the game as a collection of small, reusable, independent systems connected through thin glue code.
Each system should do one job well, expose a clear interface, and remain usable outside its original context.

### Principles

**1. Prefer modular systems over monolithic code**
Do not create large files or classes that manage unrelated responsibilities.
Split logic into focused systems such as input, movement, combat, UI, save/load, audio, and AI.

**2. Keep responsibilities narrow**
Each module, class, or function should have a single clear purpose.
Game rules, state management, rendering, and engine integration should remain separate.

**3. Use glue code only for orchestration**
Glue code should connect systems, pass data, and define flow.
It should not contain core gameplay logic, complex rules, or duplicated behavior.

**4. Design for reuse**
Systems should depend on clear inputs and outputs, not hardcoded scene objects, global state, or special-case assumptions.
A reusable system should work in multiple game modes, levels, or projects with minimal changes.

**5. Avoid oversized files**
If a file becomes difficult to scan quickly, split it.
As a guideline:

* one file should usually represent one main concept
* long files should be broken into subcomponents
* deeply mixed logic is a sign of poor structure

**6. Separate data from behavior when useful**
Configuration, tuning values, and content definitions should live outside core logic when possible.
Avoid scattering magic numbers and hardcoded paths through gameplay code.

**7. Keep dependency direction clean**
Low-level systems should not depend on high-level game-specific code.
Reusable code should stay at the bottom of the dependency tree.
Game-specific behavior should compose existing systems rather than modify them directly.

**8. Prefer explicit interfaces**
Systems should communicate through well-defined APIs, events, or messages.
Avoid hidden coupling, implicit side effects, and uncontrolled access to internal state.

**9. Optimize where it matters**
Write clear code first, but avoid wasteful patterns in frequently updated systems.
Be especially careful in update loops, pathfinding, spawning, animation, and UI refresh paths.
If code is going to be in a hot path, used often, and profiling show it to be a bottleneck, move to the rust layer.

**10. Use clear and stable project structure**
Folders and namespaces should reflect responsibility, not convenience.
Paths should be predictable and easy to navigate.

Example:

```text
/Game
  /Core
  /Systems
    /Input
    /Movement
    /Combat
    /Inventory
  /Entities
  /UI
  /Content
  /Tools
```

**11. Minimize nesting complexity**
Nested paths are acceptable when they describe ownership clearly, but avoid excessive depth.
Do not bury important code under many layers of folders, inheritance, or control flow.

**12. Name things by purpose**
Names should describe what code does, not how it happened to be implemented.
Prefer `CombatSystem`, `DamageCalculator`, `InputBuffer` over vague names like `Manager`, `Helper`, or `Stuff`.

**13. Keep functions small and readable**
A function should do one thing and be easy to understand without scrolling through unrelated logic.
If a function needs many branches or responsibilities, split it.

**14. Limit global state**
Use shared state only when truly necessary.
Prefer passing dependencies explicitly or accessing them through controlled interfaces.

**15. Make code easy to test and debug**
Systems should be isolated enough to validate independently.
Complex behavior should be traceable without requiring the entire game to run.

---

## Practical Rules

* No giant “god classes” or “god files”.
* No system should own unrelated responsibilities.
* No gameplay logic inside temporary glue code.
* No hardcoded asset or scene paths outside approved config/content locations.
* No duplicated logic when a shared system or utility is appropriate.
* Every new feature should either extend an existing system cleanly or introduce a new focused module.
