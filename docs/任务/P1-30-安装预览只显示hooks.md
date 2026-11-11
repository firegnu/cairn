# 任务：install / uninstall 的预览只显示 hooks 的改动，不打出整份配置

2026-11-11，用户让主控记成草稿任务。**草稿，没派。** 用户：「把预览只显示 hooks 也记成草稿任务」。

## 起因

11-11 给真实环境装 canter 钩子时，主控先跑 `cairn install --agent canter --dry-run`。预览把改后的整份 `~/.canter/config.json` 原样打了出来，里面有一个模型提供方的 API key，于是进了主控会话的输出和上下文（没进 Git、没进 cairn 的库）。换不换 key 用户定。项目的账里已记成经验 #51。

## 现状（主控查过代码）

`crates/cairn/src/install.rs` 的 `run`：预览是“配置路径 + 改后的整份配置 + 软链接状态”，`--dry-run` 时返回它，正式装（含 `--yes`）时也先 `print!` 它。三家都一样：

- Claude 的 `~/.claude/settings.json` 里可能有 `env` 下的令牌等；
- Codex 的 `~/.codex/hooks.json` 只有 hooks，影响小；
- canter 的 `~/.canter/config.json` 里有提供方的 `api_key`。

`uninstall` 同样。

## 要做的

`install`、`uninstall` 的预览（`--dry-run` 和正式装之前那次）只显示 cairn 要加或要删的 hooks 条目（Claude 另加那条放行 `save` 的规则），不打出配置里的其他内容；照旧显示配置路径、改不改、软链接状态。

## 怎么算做完

用户还没说。

## 主控的理解（待用户确认）

- 显示形式：按事件列出要加、要删的命令（加上 Claude 的放行规则、Codex 的 `additionalContextLimit`），或者只显示 hooks 那一段改前改后的 JSON。不显示配置里别的键。
- 写进文件的内容不变，只改给人看的那部分；备份照旧。
- 测试用合成的配置，里面放一个假的 key，核对预览里没有它。
- 动工前改 `docs/DESIGN.md` §10.3“安装流程”那句（现在只写了“先展示要改的内容”）。
