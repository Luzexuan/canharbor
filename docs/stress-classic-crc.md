# Classic 8 字节 CRC32-v1

`traffic.format="classic_crc32_v1"` 为每个 Classic data frame 增加应用层
CRC；示例见 [classic-crc32.json](../examples/stress/classic-crc32.json)。它复用现有
固定计数、阶梯、多流、adaptive、计时、跟踪和证据预算，不改变发送调度器。
本文的软件验收使用虚拟 CAN，没有操作物理设备。

## 冻结线格式与身份

```json
"traffic": {
  "format": "classic_crc32_v1",
  "run_uuid": "00112233-4455-4677-8899-aabbccddeeff",
  "source": 0
}
```

`run_uuid` 必须是小写、含连字符的规范 UUID，`source` 是 0–4294967295 的整数。
每个 flow 冻结这两个字段；同一计划及协调清单的同一声明总线上，二者组合必须唯一。
接收者使用发送 flow 的身份验证，不能以接收通道身份替代发送源。
每次新的测量更换 UUID；Web 成功启动或找回任务后为下一次执行更换 UUID。
CLI 使用计划文件中的身份，不自动改写计划。UUID/source 是 CRC 上下文，不在帧中直接携带。

线格式固定为恰好 8 个字节，标准/扩展 CAN ID 仍由计划给出：

```text
0..4  sequence: u32，大端
4..8  CRC32:    u32，大端
CRC 输入 = UTF8(run_uuid + ":" + source十进制文本) || sequence大端4字节
```

CRC 是 IEEE/zlib CRC32，reflected 多项式 `0xedb88320`，初值和 xorout 均为
`0xffffffff`。它与 FD-v1 的 Castagnoli CRC32C 不同。Python 独立复算为：

```python
import struct, zlib
body = struct.pack(">I", sequence)
payload = body + struct.pack(">I", zlib.crc32(f"{run_uuid}:{source}".encode() + body))
```

这与 CAN_test `payload(sequence, f"{run_uuid}:{source}", length=8)` 完全一致：8 字节
不含 filler。共享黄金向量保存在 [classic-crc-fixtures.json](../api/classic-crc-fixtures.json)，
覆盖不同源、序号 0/1/u32 上界；例如上述 UUID、source=0、sequence=0 的数据为
`00000000BEB65EFB`。CRC 用于检错，不是认证；它不保证抵抗恶意构造或 CRC 碰撞。

在线 verifier 先从接收 payload 取序号并按冻结身份重算 CRC，再计唯一、重复、乱序、
缺失和范围外序号。匹配 ID 的错误长度/FD data frame 记 envelope 错误；CRC 错误单独
计数，两者均不算有效唯一接收，也不会进入 `short_sequence_only`。其他 ID/宽度的流量
按无关流量处理。`run_tag` 仍参与现有计划/协调身份管理，但不占线格式、不参与 CRC。

Adaptive 每个实际探索/测量阶段使用独立 UUID：把父 UUID 当 u128，按实际阶段序号
加 1、2、… 后重新规范化；source 保持不变。UUID 溢出在预检拒绝。派生计划和指纹
包含实际 UUID，不能把上一阶段的相同序号当成本阶段有效接收。

## 证据、版本与预算

新格式报告版本为 **12**，integrity 为 `crc32_ieee_sequence_v1`；混合旧/新 flow 的
父报告也必须为 12。协调器在 commit 前拒绝不支持 v12 的节点。旧默认 Classic 仍是
tag+sequence，Classic pattern/checkpoint 和 FD 格式均保留；默认 traffic 继续省略，
旧计划指纹和原报告版本不变。旧 reader 应拒绝新格式/新事件，不能降级成旧 Classic。

`classic_crc_frame` 证据保存接收通道、原始 payload、实际 ID/extended/FD/BRS 和接收
时间，不提供可被信任的预填序号。重放独立从原始字节提取序号并重算 CRC；完成及重启
恢复要将复算结果与报告核对。旧 `sample` 事件不能冒充这个格式的原始证据。

`evidence.sample_every=1` 才保存所选接收 ID/宽度的全部 data frame，并验证完整接收
计数；`sample_every=0` 仅有聚合证据，`>1` 仅保留被采样字节，均不能声称独立重捕
全部接收。它不是全总线录制。
现有最多 **2,000 records/s**、内存和磁盘预算照常生效，原始 RX、计时、多流和队列观测
共同占用预算。六盒合计 600 TX fps 对应 3,000 peer RX/s，已超过全量 RX 的现有预算，
即使没有额外计时也会预检拒绝；不能拿抽样离线复算替代六盒全量原始证据验收。

新格式不增加 checkpoint 帧，8 字节均用于序号和 CRC。它不扩大现有计数、速率、时长
或并发采集上限，不新增请求应答/RTT，也不证明物理 ACK、仲裁、总线利用率或真实线速。

## CLI 与 Web

复用现有计划文件入口，无须新增格式专用命令：

```sh
canharbor stress validate --plan examples/stress/classic-crc32.json
canharbor stress run --plan examples/stress/classic-crc32.json --active --request-id crc-demo-001
```

上述 validate/run 仍需要已启动的本地服务，启动方式见[压测指南](stress-testing.md)。
Web 的高级设置中选择 “Classic CRC32-v1 (8 bytes)”，填写 run UUID 和 source；新增
reverse flow 会选同一 UUID 下未占用的 source，也可逐 flow 编辑。修改身份使旧预检
失效，重新验证和显式启动后才发送。不要复用示例身份作为不同实测批次的身份。
