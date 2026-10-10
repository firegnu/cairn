# 任务：记下每个项目里每家 hook 最近触发的时间；把 paddock 用到的输出定成公开约定

2026-10-10，paddock/main（paddock 的主控，Claude Code，opus[1m] / high）直接在本仓库做，没有经 cairn/main 派发。
路由：没问路由（主控自己做）。影响面按「碰要害」办：第一次改表结构、第一次升级已有数据库，所以做交叉审查。
类型：功能变更
依据：paddock 的 Cairn 面板（paddock P5-55）只知道 hook 装没装，分不出“装了但没触发”（比如 Codex 的 hook 没在 `/hooks` 里信任，面板照样打勾）；它调用的三条命令的输出也没写成约定（`docs/调研/与paddock结合.md` 列的缺口）。用户 10-10 先让 paddock/main 把两项需求交给 cairn/main（“那两项都提吧， 你直接派活给cairn的主控，让他去做。你的活你自己做”），cairn/main 分析后给了 A（按项目，要升表结构）、B（按 agent 全局，不动表）两条路并在等用户选；用户取消了那次提问，改口：“我取消了，我觉得你直接再cairn中干吧，之后更新他的handoff文件，以便下次能够正确识别最新的情况。另外我觉得问题不要最小化操作。就是一次解决。之前cairn中的数据我认为不重要。你理解我意思吗？”
说明：本仓库 AGENTS.md 写“主控不自己写功能代码”，这一件照用户上面的话由 paddock/main 自己写；别的规矩（隐私、不改用户真实配置、测试隔离、不用 Python）照旧。

## 在哪里干活
- worktree：`/Users/firegnu/Developer/personal_projs/cairn-worktrees/f2-hook-seen`，分支 `f2-hook-seen`（已从 main 建好）。
- 编译：cargo 命令前加 `CARGO_TARGET_DIR=$HOME/Developer/personal_projs/cairn-worktrees/.target`。

## 要做的
1. **按项目、按 agent、按事件记最近触发时间**（不走只改 `status.rs` 的 B）：表结构升到版本 2，加 `hook_seen`（项目，agent，事件 → 最近一次时间）；四种 hook 事件处理时各自更新自己那一行；`cairn status --json` 的 `agents.<名字>.last_seen` 按当前项目报四个事件的时间，没有是 `null`；`cairn status` 的文字输出也加一行。
2. **升级已有数据库**：版本 1 的库在读写打开时升到版本 2，只加表，旧表旧数据不动；只读打开不升级、照样能读。用户说旧数据不重要，但 owlet、paddock 两个试点项目的接续记录现在只在这个库里，所以不清库（主控的判断，已告诉用户）。
3. **公开约定**：把 paddock 用到的三条命令的输出写进 DESIGN §7.1，定“只加不改”。
4. DESIGN §6.2（版本 2）、§8.7（记法和限制）；README 里 `status` 一行；版本号 0.1.0 → 0.2.0（能从 `cairn --version` 看出装的是哪一版）。

## 怎么算做完
- 用户没给验收原话；主控照上面四条自查。
- 测试：升级（版本 1 的库只读能读、读写打开后升到 2、旧数据都在、再开一次不再变）；两个进程同时升级；四种事件各记各的、别家和别的事件不受影响、后一次覆盖前一次；`CAIRN_DISABLE=1` 和未采用时不记；版本 1 的库上 `status` 全报 `null` 且不升级。
- `cargo test --all-targets` 和 `cargo clippy --all-targets -- -D warnings` 通过。
- Codex 只读交叉审查到“可以合并”。

## 不要做
- 不读用户真实的会话记录和真实的 cairn 数据库内容；测试只用合成材料和隔离目录。安装新版前只按文件整份备份数据库，不打开看。
- 不改 `~/.claude/settings.json`、`~/.codex/hooks.json`（hook 命令走稳定软链接，换程序不用改配置）。
- 不动 `list --json`、`show --json` 分节这些 paddock 以后才要的东西。
