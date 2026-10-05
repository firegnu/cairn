# 任务：2b 作用域与 Git 事实（crates/cairn/src/scope.rs、facts.rs）

2026-10-05，cairn/main（主控）交给 cairn/dev-scope（Codex，常规：gpt-6-astra / high）。
路由：常规 / 交叉审查不要 / 影响面：改行为（路由：档拿不准（常规 0.53、重 0.47）、交叉审查拿不准、影响面拿不准；主控按规则定）
类型：功能变更
依据：本轮只做"当前目录属于哪个项目、哪条工作线"的判定，Git 事实采集，以及现场对比的计算和措辞；不读写数据库，不做渲染整段注入文字。
提示：围绕已确认的使用目标完成变更，优先沿用现有机制。
你是被委派的 agent：照本文件做，不要再开别的 agent。

## 先读
- `AGENTS.md`（规矩一节，尤其"Git 信息通过调用 git 命令取得，调用要设超时"）
- `docs/DESIGN.md` §3 术语（项目、工作线）、§5、§8.2 save 进程里的第 1、3 步、§11.2
- `docs/实施计划.md` 阶段 2 的 2b 一行

## 在哪里干活
- worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/p2b-scope`，分支 `p2b-scope`（已从 main 建好）。
- crate 骨架已在 main 建好：`crates/cairn`，`lib.rs` 已声明 `store`、`scope`、`facts` 三个模块。
- 只动：`crates/cairn/src/scope.rs`、`crates/cairn/src/facts.rs`（都可以拆成同名目录下的子模块；两者共用的"带超时调用 git"放在其中一个里，另一个引用）、`crates/cairn/tests/` 下以 `scope` 或 `facts` 开头的测试文件、`crates/cairn/Cargo.toml` 的 `[dependencies]` / `[dev-dependencies]`、`Cargo.lock`，以及本任务文件末尾的完成记录。
- 并行任务：cairn/dev-store 在分支 `p2a-store` 上写 `store.rs`。你不要动它和 `lib.rs`。两边都可能往 `Cargo.toml` 加依赖，合并冲突由主控处理。
- 编译：所有 cargo 命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。

## 要做的
- **作用域**（§3、§5）：给一个目录，判定
  - 项目键：`git rev-parse --path-format=absolute --git-common-dir` 的规范路径；
  - 工作线：`git rev-parse --show-toplevel` 的规范路径；
  - 不在 Git 里：项目键和工作线都用这个目录的规范路径，并标明"不在 Git 里"。
  - git 不存在、超时、其他出错：返回错误，由调用方决定（hook 会放行），不要猜。
- **Git 事实**（§8.2 第 3 步）：HEAD（提交号；空仓库没有 HEAD 也要能处理）、分支（detached 时为空）、本地上游 ref 名及 ahead / behind、工作区已暂存 / 未暂存 / 未跟踪文件数、采集时间。结果可以序列化成 JSON（2c 会把它存进 `records.facts`）。
- **调用 git 的规矩**：
  - 每次调用都有超时（建议默认 2 秒，可由调用方传入），超时就杀掉子进程并返回错误；
  - 加 `--no-optional-locks`（Codex 沙箱里不能写 `.git/index.lock`）；
  - 输出用机器可读格式（例如 `status --porcelain=v2 -z`），不依赖用户的 git 配置和语言环境；
  - 不 fetch，不做任何联网操作，不修改仓库。
- **现场对比**（§11.2）：给"记录时采集的事实"和工作线目录，算出现在和记录时的差别，生成几行固定措辞的文字。措辞照 §11.2 原文：
  - 记录时的 HEAD 还在当前分支历史上：`HEAD 比记录时多 n 个提交`；不在：`记录时的 HEAD 已不在当前分支历史中`；
  - 上游一律写成 `相对本地 upstream ref <名> ahead a / behind b（只基于本地 ref，不代表远端实际状态）`；
  - 工作区写成 `当前有 x 个已暂存 / y 个未暂存 / z 个未跟踪文件`；
  - 最后必须附上 `现场变化不说明记录叙述过时。`
  - 不出现"已推送""远端已有""崩溃""丢了"这类说法。

## 怎么算做完
以下验收条件经用户 2026-10-05 同意，取自实施计划 2b 的"建议验证"：
- 临时仓库加 worktree：仓库的子目录、另一个 worktree，都判定为同一个项目；工作线各不相同。
- 记录时的 HEAD 不在当前分支历史中时，措辞正确。
- 上游对比只用本地 ref（测试里不配真实远端，或者用本地路径做远端且不 fetch）。
- 非 Git 目录能正确处理。

验证只做这些：
- 上面每一条各有一个测试，外加 git 超时会返回错误的一条测试。测试只用临时目录里的合成 Git 仓库，并把 git 的全局 / 系统配置隔离掉（例如 `GIT_CONFIG_GLOBAL`、`GIT_CONFIG_NOSYSTEM`、临时 `HOME`）。
- `cargo test --all-targets` 和 `cargo clippy --all-targets -- -D warnings` 各跑一次并通过。
- 觉得不够，在回复里说，不要自己加。

## 不要做
- 不要读写数据库，不要动 `store.rs`、`lib.rs`、`main.rs`、`tools/`、`docs/`（本任务文件的完成记录除外）。
- 不要在用户真实的仓库里跑测试，不要读真实会话记录、各工具的记忆文件。实现、测试、辅助脚本都不要用 Python。
- 不要引入 libgit2 这类库，Git 信息一律调用 `git` 命令。依赖只用成熟、活跃维护的库。
- 不要按项目名或路径批量杀进程（`pkill -f cairn`、`pkill -f git` 这类）。杀超时的 git 子进程用你自己拿到的子进程句柄。
- 遇到 DESIGN 说不通的地方，停下来报告，等主控决定。
- 不合并到 main，不推送。只在 `p2b-scope` 分支上提交。

## 做完
在本文件末尾追加「## 完成记录」（在你的分支里提交）：做了什么、验证了什么、拿主意的地方、没做的事，各几句话。回复里只写这几样，加上有没有要主控决定的事。命令都在前台跑完，全部做完后，回复最后一行写 DONE。
