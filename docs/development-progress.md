# CANHarbor 开发进度

记录日期：2026-11-24（Asia/Shanghai）。范围与验收定义以[原子开发路径的 53 个单元](development-path.md#原子开发单元)为准；该表的能力描述是 2026-11-17 基线，本页记录后续实施事实。

首批完成 7 个单元的软件实现：H01、H02、H08、ST01、ST02、TX01、TX09，均有独立功能提交及 Draft PR。其余 **46/53 个单元未实施**，不能把基础组件完成计为下游业务功能、组合验收或产品硬件支持已完成。

## 分支与交付边界

规划分支为 codex/atomic-development-roadmap，共同开发基线是 [094eba92861b00a5435c1a5b1c96e3c56e0c38c6](https://github.com/Luzexuan/canharbor/commit/094eba92861b00a5435c1a5b1c96e3c56e0c38c6)。下表七个功能分支均直接以该提交为父提交，各只有一个完整原子提交；没有跨 feature 分支依赖。后续功能依赖仍按原 53 项表执行。

Draft PR 的目标为规划分支，目前均未合并。main、origin/main、upstream/main 仍指向 [6bde320473ba1d5e07231dee8baa661fbf02c84a](https://github.com/Luzexuan/canharbor/commit/6bde320473ba1d5e07231dee8baa661fbf02c84a)，本轮没有修改 main。

组合检查在独立、detached HEAD 的临时 worktree 中逐项 cherry-pick；其提交哈希是临时组合产物，不替代下表原始 feature commit，也未合入 main。组合测试通过与各 Draft PR 独立通过分别记录。规划分支目前也没有获得这些功能代码。

## 首批单元

“软件验证”中的本地结果来自相应实施记录和 PR；独立复核为只读源码检查。CI 状态另列，不能以本地测试替代尚未结束或失败的 CI。

| 单元 / 状态 | 实际交付行为与范围 | 独立分支 | 固定 feature commit | Draft PR | 软件验证 | 硬件待验 |
| --- | --- | --- | --- | --- | --- | --- |
| H01：文档完成，待合并 | 统一软件测试、有限烟测、受控台架与产品支持等级；补可追溯记录字段，保留 2026-11-05 SLCAN 有限烟测和空正式矩阵。 | codex/h01-hardware-validation | [14c5df060826f7cb07f21d80e420013d98828e3b](https://github.com/Luzexuan/canharbor/commit/14c5df060826f7cb07f21d80e420013d98828e3b) | [#1](https://github.com/Luzexuan/canharbor/pull/1) | 51 个本地文件链接、既有 CLI 命令及 diff 检查；纯文档未新增执行测试。 | 本项不提升任何硬件等级；历史烟测范围保持原样。 |
| H02：软件完成，待合并 | 严格、有界的 v1 多 gs_usb/SLCAN 启动 manifest；全量预检、同快照 USB 别名去重、fresh UUID；保留旧参数，部分打开及服务启动失败等待清理并保留 unknown。 | codex/h02-multiple-adapters | [ad5dbef3fb1c65033f5fde94798b82cc6c119c15](https://github.com/Luzexuan/canharbor/commit/ad5dbef3fb1c65033f5fde94798b82cc6c119c15) | [#6](https://github.com/Luzexuan/canharbor/pull/6) | 本地 CLI 53 passed / 3 ignored；默认构建、SLCAN/默认 Clippy、格式与本地化检查通过；启动与清理路径已独立复核。修复 macOS 测试目录后，三平台 CI 通过。 | 两台真实适配器同时注册、路由隔离、收发、停止及重开仍待验；不会自动配置或启动 CAN。 |
| H08：软件完成，待合并 | 显式绝对 Unicode DLL 路径、受限依赖搜索、进程驻留、文件身份别名去重、共享 channel claim 与来源观测；提供 loader 底座。 | codex/h08-windows-loader | [0d689b659eafed515b27844ade5f57a4fb92cfef](https://github.com/Luzexuan/canharbor/commit/0d689b659eafed515b27844ade5f57a4fb92cfef) | [#4](https://github.com/Luzexuan/canharbor/pull/4) | Windows driver/platform 共 17 项测试、Clippy 及 Unix 交叉检查通过；合成 DLL 覆盖缺符号、坏映像、真实 x86 DLL 被 x64 进程拒绝、别名/claim、驻留期间替换尝试和来源一致性。 | 尚未接入或验证 PCAN/HCanbus 厂商设备；H08 不等于 H09/H11 后端完成。 |
| ST01：软件完成，待合并 | 离线生成 2–8 通道全互发计划，支持固定/阶梯总速率、唯一 ID/tag 与 alias；六节点可表达 30 条非自环链路，并在输出前编译预算。 | codex/st01-all-to-all-plan | [a6dcbd7317943f56bf9443e0cdd8e0c161d04e60](https://github.com/Luzexuan/canharbor/commit/a6dcbd7317943f56bf9443e0cdd8e0c161d04e60) | [#3](https://github.com/Luzexuan/canharbor/pull/3) | 43 项生成器、多流、CLI、本地化和 completion 测试通过；SLCAN 目标 Clippy、格式及 diff 检查通过。生成无需服务或硬件。 | 仅交付计划生成；不代表六盒实机互发、稳定吞吐或全量 RX 落盘已经验收。 |
| ST02：软件完成，待合并 | Classic 单帧 8 字节序号 + IEEE CRC32-v1，冻结 run UUID/source；贯通原始字节重算、v12 报告、replay、落盘恢复、CLI/Web 和生成契约。旧 Classic/checkpoint/FD 及指纹保留。 | codex/st02-classic-crc | [59edc7e1539c4b7ef2668e5ce433a2b51922728c](https://github.com/Luzexuan/canharbor/commit/59edc7e1539c4b7ef2668e5ce433a2b51922728c) | [#7](https://github.com/Luzexuan/canharbor/pull/7) | 完整 stress 套件通过；最终 CRC 集成 5 项、服务恢复、CLI 预检/协调、Playwright 2 项及 build/API/i18n/format/Clippy 均通过。独立 zlib 核验 6 个黄金向量，raw replay 路径已独立复核。 | 未实机。sample_every=1 才能全量 RX 离线重算；现有 2,000 records/s 上限仍保留，不能承载六盒 600 TX fps 对应的 3,000 peer RX/s 全量证据。 |
| TX01：软件组件完成，待合并 | 公开纯 Rust 有界 response matcher：ID/位宽/帧型/长度/字节掩码/可选序号；输出匹配、无关、重复、歧义。 | codex/tx01-response-matcher | [4e6cf6c91aa6012ed537f7a7e6b9764abfaf8806](https://github.com/Luzexuan/canharbor/commit/4e6cf6c91aa6012ed537f7a7e6b9764abfaf8806) | [#5](https://github.com/Luzexuan/canharbor/pull/5) | 21 项 task 测试、文档示例、Clippy、格式与 diff 检查通过；越界、窗口耗尽、退役回复、序号复用和无序号歧义有针对性覆盖。 | 无 Runtime、服务、时钟、deadline 或健康动作，不是完整业务事务。无序号歧义不能自动判为总线丢帧或成功；后续 TX02 等负责执行策略。 |
| TX09：软件完成，待合并 | python-can 新增累计健康快照及有界诊断出口，分别记录 stream gap、本地缓冲丢弃、overflow/fault 和未知损失；保持 recv 与控制语义。 | codex/tx09-python-observability | [d66c2fa92a58590f6f9fab385a88f6cccdfdf208](https://github.com/Luzexuan/canharbor/commit/d66c2fa92a58590f6f9fab385a88f6cccdfdf208) | [#2](https://github.com/Luzexuan/canharbor/pull/2) | 8 项 fake-stream 测试及 Rust → C ABI → Python 虚拟集成通过，python-can 4.6.1；已独立复核队列边界、race 和 unknown_loss。 | 真实慢消费者及适配器 overflow 对照仍待验；主机缓冲丢弃不等于物理总线丢帧。 |

## CI 与本地环境记录

以下为 2026-11-24 核对的 PR head 对应 Actions 状态；它们是独立分支检查。提交、Draft 状态与运行结果分别通过 Git/GitHub 元数据核对。

| PR / 单元 | 已核对的运行 | 结果 |
| --- | --- | --- |
| #1 / H01 | [CI 45317398727](https://github.com/Luzexuan/canharbor/actions/runs/45317398727) | completed / success |
| #2 / TX09 | [CI 45317414452](https://github.com/Luzexuan/canharbor/actions/runs/45317414452) | completed / success |
| #3 / ST01 | [CI 45317415310](https://github.com/Luzexuan/canharbor/actions/runs/45317415310)、[Stress 45317418483](https://github.com/Luzexuan/canharbor/actions/runs/45317418483) | 两者 completed / success |
| #4 / H08 | [CI 45317421464](https://github.com/Luzexuan/canharbor/actions/runs/45317421464) | completed / success |
| #5 / TX01 | [CI 45319032069](https://github.com/Luzexuan/canharbor/actions/runs/45319032069) | completed / success |
| #6 / H02 | [CI 45325056005](https://github.com/Luzexuan/canharbor/actions/runs/45325056005) | completed / success |
| #7 / ST02 | [CI 45325177577](https://github.com/Luzexuan/canharbor/actions/runs/45325177577)、[Stress 45325177732](https://github.com/Luzexuan/canharbor/actions/runs/45325177732) | 两者 completed / success；包含 Unix/Windows 的最终新测试和 macOS WebKit 兼容检查。 |

H02 [首轮 CI 45319710876](https://github.com/Luzexuan/canharbor/actions/runs/45319710876) 曾因 macOS 测试目录超过 Unix socket SUN_LEN 失败，按既有惯例使用短 /tmp 路径后，[CI 45320932682](https://github.com/Luzexuan/canharbor/actions/runs/45320932682) 通过。随后只调整新增翻译键的插入位置，消除 H02/ST01 组合后分别撤回的文本冲突，仍保留为一个功能提交。

ST02 [首轮 CI 45321542582](https://github.com/Luzexuan/canharbor/actions/runs/45321542582) 的 Unix api:check 曾失败。根因是本地共享 node_modules junction 使 esbuild 把外部真实依赖路径写入生成文件，本地同路径检查因此未发现差异。已在独立依赖目录复现该失败，重新 frozen install 并生成后仅修正两处路径；没有修改生成器或放宽校验。最终生成时各 worktree 使用自己的实际 node_modules 目录。对应旧 head 的 [Stress 45321542601](https://github.com/Luzexuan/canharbor/actions/runs/45321542601) 通过，仍以最新 head 的检查结果作为交付依据。

下一轮 [CI 45323156774](https://github.com/Luzexuan/canharbor/actions/runs/45323156774) 的新 Web 测试在 Unix 读取 CDP 响应体时失败；全部 Rust/Clippy/API、Windows、打包及 [Stress 45323156787](https://github.com/Luzexuan/canharbor/actions/runs/45323156787) 通过。Ubuntu trace 显示唯一预检 POST 返回 200、页面已显示验证成功，初始加载后没有导航或请求中止。仅测试改为按既有模式先读取真实 route.fetch 响应，再原样交给页面；保留 200、请求 traffic、v12、预检可见与编辑后失效全部断言，没有增加重试或修改产品逻辑。

本地使用 Rust 1.99.1、MSVC、Bun 1.4.2，Rust 构建并发限制为 CARGO_BUILD_JOBS=2。H08 的 x86 DLL fixture 另需 i686-pc-windows-msvc target；Python 虚拟集成需实际安装 python-can，缺包时原测试可跳过，跳过不能记作已验证。

Git HTTPS 在最后一次推送时连接重置，但 GitHub API 可用。ST02 修正版通过 Git 对象 API 上传，逐项确认 blob、tree 和 commit 与本地完全一致后，以旧 SHA 为条件更新分支。没有重建不同作者或父提交的替代提交。已核对全部七个功能分支的远端 SHA 与本地一致；origin 的获取范围由单分支扩为全部分支，便于后续正常获取这些独立分支。

临时六单元组合的首轮 workspace tests 在 FFI C 示例处因找不到 clang 失败：crates/canharbor-ffi/tests/abi.rs 中调用编译器未成功启动。原始终端输出留在本次聊天，未另存首轮日志。不能把这次失败改记为全套通过，也不能据此断言功能代码存在缺陷。

随后从官方 LLVM 20.1.8 安装包纯解包补齐临时编译器，未全局安装。真实 C 示例的编译、链接与执行通过；七单元组合的一轮完整 Rust 检查也包含该例子和 H08 真实 x86 DLL：**444 passed、12 ignored、0 filtered**。Windows workspace Clippy（all-targets，-D warnings）、Web build、API 生成检查、本地化和格式检查通过。12 个 ignored 保留既有显式实时、性能或长稳门禁，不记作本轮完整执行。之后仅修正生成路径、翻译键位置与测试观测方式，分别补跑 API/build、CLI 翻译与 Playwright 检查，最终 feature head 的全套回归由上表 CI 执行。

独立 CLI 组合烟测使用内建 virtual 后端：离线生成 2 通道全互发计划，转换为同 UUID、source=0/1 的 CRC 格式，再显式执行合计 20 fps × 2 秒。实际 40 TX、40 peer RX，40 条原始记录独立 zlib 校验均通过，CRC/缺失/重复/乱序/unknown/skipped 为 0。停止本测试创建的服务进程并重开同 runtime 后，原 run recovery=verified、计数不变、无新 run。该测试不替代真实适配器验收。

本机证据保存在 C:/Users/17700/canharbor-worktrees/.runtime/validation/：combined-six-rust.log 是早期只跳过 C 示例的结果，combined-seven-rust.log 是完整组合结果，combined-seven-clippy.log 和 combined-seven-web.log 记录对应检查。st02-local-junction-repro.log 保留独立复现生成路径问题的预期失败；修复后 combined-seven-generation-fixed.log 的 API/build/CLI 构建和 combined-seven-playwright.log 的 2 项 Web 测试通过。生成路径修复不改变 Rust 逻辑，因此没有重复整个 Rust 套件。

st01-st02-20261124-114808-8eedb51f/ 保留首次组合烟测的计划、前后报告、原始 CRC 核验及全部 CLI 输出；生成物修正后的第二次证据在 st01-st02-20261124-120425-6124631c/，消除翻译冲突及修复测试后的最终第三次证据在 st01-st02-20261124-121721-403ab33f/。三次均为 40 TX / 40 peer RX、CRC=0、重启 verified，前轮证据未覆盖。最终临时组合提交是 ba637a3f194a4937b9c9a89e20c56e5a077cb6c5，所测二进制 SHA256 为 7b7de3b3e2f4163bde4e04ee24ff6b57699516ee36a7825117a0a16e755cc73b；combined-final-cli-build.log 和 combined-final-playwright.log 记录最后的构建与两项 Web 检查。所有自建服务进程均已结束。这些是本地执行产物，未将整个证据目录发布到仓库。

## 可重跑的软件检查

先进入对应单元的 worktree；下面均为软件、虚拟服务或合成 DLL 检查，不需要连接物理 CAN。命令是复验入口，不表示本页编辑时重新运行了全部测试。Windows 需在 MSVC 可用的开发环境运行，Python 解释器指向已装 python-can 的隔离环境。

~~~powershell
$env:CARGO_BUILD_JOBS = '2'
cargo +1.99.1 fmt --all --check
git diff --check
~~~

| 单元 | 在对应 feature worktree 执行的重点命令 |
| --- | --- |
| H01 | 按提交中的文档核对本地链接、历史烟测与 CLI 参数；git diff --check。纯文档不新增测试。 |
| H02 | cargo +1.99.1 test -p canharbor-cli --features slcan --locked；cargo +1.99.1 test -p canharbor-cli --no-default-features --locked；cargo +1.99.1 clippy -p canharbor-cli --all-targets --features slcan --locked -- -D warnings。Unix 别名/PTY fixture 需 Unix runner。 |
| H08 | cargo +1.99.1 test -p canharbor-driver -p canharbor-platform --features canharbor-driver/dynamic --locked；cargo +1.99.1 clippy -p canharbor-driver -p canharbor-platform --all-targets --features canharbor-driver/dynamic --locked -- -D warnings。真实 x86 fixture 需前述 target。 |
| ST01 | cargo +1.99.1 test -p canharbor-stress --test templates --test group --locked；cargo +1.99.1 test -p canharbor-cli --features slcan --test plan_generation --test localization --locked。 |
| ST02 | cargo +1.99.1 test -p canharbor-stress classic_crc --locked；cargo +1.99.1 test -p canharbor-stress --test classic_crc --locked；cargo +1.99.1 test -p canharbor-server --test stress classic_crc --locked；bun run --cwd web api:check；bun run --cwd web typecheck；bun run --cwd web test:e2e classic-crc.spec.ts。Web 测试需匹配当前源码的 web-ui 二进制与 Playwright 浏览器。 |
| TX01 | cargo +1.99.1 test -p canharbor-tasks --locked；cargo +1.99.1 clippy -p canharbor-tasks --all-targets --locked -- -D warnings；执行提交文档中的 matcher 示例。 |
| TX09 | 将 PYTHONPATH 指向当前 worktree 的 python/src 后执行 python python/tests/pythoncan_observability.py；设置 CANHARBOR_TEST_PYTHON 为隔离 Python 绝对路径，再执行 cargo +1.99.1 test -p canharbor-ffi --test python --locked。 |

组合检查命令如下，需在组合 worktree 中执行。临时 clang 环境就绪后已完整通过；初次缺工具的失败事实保留在上文。

~~~powershell
# 本机临时 LLVM/MSVC 环境；其它主机使用自己的 clang/MSVC 环境。
. 'C:/Users/17700/AppData/Local/Temp/canharbor-tools/enable-msvc-clang.ps1'
$env:CANHARBOR_TEST_PYTHON = 'C:/Users/17700/canharbor-worktrees/.runtime/tx09-python/Scripts/python.exe'
cargo +1.99.1 test --workspace --exclude canharbor-peak --exclude canharbor-kvaser --features canharbor-cli/slcan,canharbor-cli/web-ui,canharbor-driver/dynamic --locked
~~~

## 回滚与数据保留

未合并的 feature 可以保留分支/PR 而不选入交付。若已纳入某交付分支，先停止服务及相关测试、等待关闭结果，再在该交付状态上创建回滚分支并用一个 revert 提交撤销单元。下例演示 H02 独立提交的具体回滚，不是本次已执行的命令：

~~~powershell
git switch -c codex/revert-h02 codex/h02-multiple-adapters
git revert ad5dbef3fb1c65033f5fde94798b82cc6c119c15
~~~

其它单元使用上表对应的完整 feature commit。若回滚的是组合/已合入分支，应从该分支当前交付状态建立回滚分支，不切回示例中的独立 H02 状态。当前七个单元互无硬依赖，可分别撤回；以后若已有下游依赖，必须按原 53 项表逆序撤回，例如先 TX03 再 TX02 再 TX01、先 H03 再 H02，不以 reset 或删除历史证据代替回滚。

已在独立临时 index 上对七项分别执行反向补丁的三方应用检查，真实工作区/index/分支未变。初轮 H02/ST01 因新增翻译键相邻而有文本冲突，记录保存在本机 .runtime/validation/rollback-index-check-20261124-120815-39a78e3e/。将 H02 的键移到既有服务参数组后重新组合，各单元均无合并冲突；最终七项反向补丁全部通过、无冲突并生成各自 tree hash，结果位于 rollback-index-check-20261124-121701-15dbdeb4/。这证明当前组合上的补丁可应用性，不替代回滚后的编译、功能与证据读取验收，也不保证未来任意改动后的组合永无文本冲突。

回滚保留 manifest 原件、烟测/失败记录、录制及压测证据。H02 可恢复旧单设备入口；TX09 撤回新增观察出口后，不能将旧版本缺失健康数据解释成零丢失。ST02 的新 v12 证据仍由支持该格式的 reader 只读保留，回退旧二进制使用独立 runtime 目录；不把新证据改写为旧格式通过结果。TX01 本身不持久化事务状态，撤回 matcher 不意味着允许重发未确认请求。

## 下一步与实际门禁

其余单元仍按[原 53 项开发表](development-path.md#原子开发单元)规划，本页不复制整张表。下列是近期 P0 路线中的真实前置关系，不把共享文件或实现方便写成新增硬依赖：

- H06 固件 profile、H04 缓存物理身份、ST03 精细调度、ST04 错峰、EV01 证据批量写入各自无硬前置，可独立实施。H06 先通过保存交互 fixture，再逐步单盒、两盒确认方言，不能只放宽版本字符串。
- H03（COM/USB 身份选择）依赖 H02；H07（配置请求与读回证据）依赖 H04。H08 的 DLL loader 完成后仍需另行实现厂商后端；例如 H09 的前置是 H02、H04、H07、H08。
- EV02 全量 RX 档位依赖 EV01 吞吐证据；公共证据导出 EV05 之后才是独立 verify EV06。2,000 records/s 预算在这些任务完成前保持原状。
- TX02 Runtime 单次事务依赖 TX01，TX03 服务入口依赖 TX02；长期事务 TX05 还依赖 TX03 与 TX04，持久证据 TX08 再依赖 TX05、EV06。完成 matcher 不能跳过 deadline、隔离、取消、权限及 unknown 的执行验收。
- ST07 六节点参考验收依赖 EV02、EV06、H02、H06、H07、ST01、ST02、ST03、ST04、ST06。首批七单元仅覆盖其中一部分，不能提前宣称六盒参考套件通过。

本轮没有连接或操作真实 CAN 设备。新功能的物理收发、FD/BRS、拔插、厂商驱动、长稳与外部测量仍待验；历史 2026-11-05 SLCAN 有限烟测继续保留，不能推广为所有硬件均已验证。

六盒 H12 实机门禁需要 EV06、H01、H02、H03、H04、H05、H06、H07、ST07，并提交同设备/固件/系统条件的 600 秒、360,000 TX 与 1,800,000 peer delivery 原始证据和复算。gs_usb H15 及 Windows PCAN H13 按各自设备条件和前置验收，某条硬件路线缺设备不能阻塞其它路线的软件开发。应用 CRC、驱动接受、虚拟 CAN 通过及主机观察均不能替代物理层测量或产品支持等级。
