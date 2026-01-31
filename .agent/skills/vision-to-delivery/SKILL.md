---
name: vision-to-delivery
description: Transform a high-level North Star vision into a delivered reality through user-centric design, strategic roadmap planning, and rigorous TDD execution using OpenSpec.
---

# Vision to Delivery (北极星交付流)

这是一个**从愿景到落地**的完整周期 Skill。不仅仅是写代码，更是**产品设计 + 架构规划 + 精益执行**的结合。

## 核心原则 (Leadership Mindset)

1. **建设大教堂，而不是搬砖**: 每一次敲击键盘，都是为了实现最终的“北极星”。不要在细节中迷失方向。
2. **以用户为中心**: 愿景通常是模糊的（如“做一个好用的后台”）。你的职责是将其翻译为**令用户尖叫的体验**（设计、交互、美学）。
3. **依赖先行**: 规划时，必须识别核心路径。先造地基，再起高楼。
4. **TDD 闭环**: 没有测试的代码由用户来测试。我们不接受不可靠的交付。

---

## Workflow（执行流程）

### Phase 0: 北极星对齐 (Vision Alignment)

**输入**: 用户的一句话愿景 (e.g., "我想做一个像 Linear 一样的任务看板")。

1. **项目初始化 (Init)**:
    *   **渐进式检查**: 检查是否存在 `openspec.config.yaml` 或 `.openspec` 目录。
    *   如果已存在：跳过初始化，直接进入下一步。
    *   如果**不**存在：执行 `openspec init`，确保基础环境就绪。

2. **用户体验推演 (UX Discovery)**:
    *   思考：这个愿景背后的真实用户场景是什么？
    *   行动：提出 2-3 个关键的“Wow Moments”（惊喜点）。
    *   产出：**Product Specs (产品规格)** - 描述最终形态，而不是实现细节。
    *   **关键动作**:
        *   检查根目录是否存在 `project.md`。
        *   **如果不存在**: 创建它，并将详细愿景写入。
        *   **如果已存在**: 读取现有内容，确认本次愿景是否与现有北极星冲突。如果不冲突，将本次愿景作为增量更新添加到 `project.md` 的 [Changelog/Updates] 章节。

3. **技术可行性分析**:
    *   评估实现该愿景需要的技术栈、核心难点。
    *   确认是否需要引入新的架构组件。

### Phase 1: 战略规划 (Roadmap & Breakdown)

**输入**: 明确的产品规格。

1.  **拆解 Change (Atomicity)**:
    *   将大目标拆解为 `OpenSpec Change` 粒度。
    *   每个 Change 应该足够小，专注于一个特性或模块，但必须是**完整可工作**的。
2.  **依赖排序 (Sequencing)**:
    *   绘制依赖图：`Infrastructure` -> `Core Logic` -> `API` -> `UI`。
    *   **规避风险**: 把高风险、不确定的部分放在前面验证。
4.  **产出 Roadmap**:
    *   在根目录生成/更新 `ROADMAP.md`，列出所有待执行的 Change 列表。
    *   可以使用 `openspec list` 查看当前已有的 Changes。

### Phase 2: 战术执行循环 (Execution Loop)

**对于 `ROADMAP.md` 中的每一个 Change，重复以下步骤：**

1.  **North Star Check (校准)**:
    *   在开始前，问自己：*“这个 Change 如何服务于最终的北极星愿景？”*
    *   如果发现当前的计划偏离了愿景（为了做而做），立即修正计划。

2. **Change 初始化**:
    *   **检查**: 是否已存在 `openspec/changes/<change-name>` 目录？
    *   **若不存在**: 运行 `openspec new change <change-name>`。
    *   **若已存在**: 检查该 Change 的状态 (`current_status`)。
    *   生成 Proposal, Specs, Design, Tasks (使用 `/opsx:ff` 或手动)。

3. **TDD 开发 (Red-Green-Refactor)**:
    *   **Red**: 为当前 Task 编写一个**失败的测试** (Unit/Integration/E2E)。
    *   **Green**: 编写实现代码，使测试通过。
    *   **Refactor**: 优化代码结构，保持优雅。
    *   *重复直到所有 Task 完成。*

4.  **完整验证**:
    *   运行全量测试套件，确保没有回归问题。
    *   手动验证（如果包含 UI）。

5.  **归档 (Archives)**:
    *   运行 `openspec archive <change-name>`。
    *   将 Change 合并入主分支/规格库。

### Phase 3: 交付与回顾 (Delivery)

1.  所有 Change 完成后，回顾 `ROADMAP.md`。
2.  生成最终的 `walkthrough.md`，展示从愿景到现实的演变过程。
3.  使用 `openspec list --specs` 查看最终生成的完整规格。
4.  庆祝！🎉

---

## 示例 Prompt

**用户**: "我想给 OpenSpec 增加一个基于 Git 的版本控制插件。"

**Agent 响应**:
> 收到。这是一个很棒的愿景。作为负责人，我不仅要实现它，更要让它**甚至比 Git 原生体验更好**。
>
> **Phase 0 分析**: 用户不仅要是版本控制，更想要的是“无感知的快照”和“一键回滚的安心感”。
> **Phase 1 规划**: 
> 1. `git-adapter-core` (底层封装)
> 2. `snapshot-mechanism` (自动触发逻辑)
> 3. `cli-integrate` (集成到 openspec change 命令)
>
> 让我们开始 Phase 2，从 `git-adapter-core` 做起...
