# g1-acq

[![Telegram](https://img.shields.io/badge/Telegram-AI羊毛交流群-26A5E4?logo=telegram&logoColor=white)](https://t.me/muihbb)

> 💬 **AI羊毛交流群**:https://t.me/muihbb —— 交流 AI 优惠、订阅羊毛

Google One（iOS）订阅入口的商品 ID 改写与采集工具，适用于 **Shadowrocket / 小火箭**。

**v7.9.0 主模块为 G1 ACQ SKU 管理。** 现有[本地报告页](https://one.google.com/g1-sku-report)升级为 **Google One SKU 管理**，支持添加、编辑、勾选启用、逐条保存、删除映射和恢复默认；采集列表可预填源，源与目标输入框提供已采集 SKU 候选。保存留在手机本地，**下一次 `/ai?` 响应才使用新规则**，已打开或缓存的方案页需重新进入。

默认模式 `discount75` 保留 20TB / 30TB 三个月 75p 和原始 iOS 2TB → `g1.2tb.ai.12months_ip_90p`；保存手动规则后为 `custom`。**75p = 75% off、实付 25%、2.5折**。当前改写运行版本为 `g1-acq-v7.9-mappings`，保留原始 SKU 采集、最近 10 次响应、重复诊断与 JSON 导出，无需另装扫描器；完整步骤见 [SKU 管理、Pro 12个月优惠测试与采集](docs/pro-annual-test.md)。

采集只读取手机实际收到的匹配响应，**不会主动查询全量 SKU**；旧 ID 再次出现时累加次数。字符串改写与 App Store 商品返回分别记录，商品名称、周期、初次与续费金额按收银台实际显示核对，权益按 Google One 账户实际状态记录。

## 主模块安装地址

**G1 ACQ SKU 管理 — 本地手动映射，保留默认 75p 与 2TB 目标**

```text
https://cdn.jsdelivr.net/gh/crhywun/g1-acq@v7.9.0/g1-acq-ultra-discount.sgmodule
```

| 项目 | 当前主入口 |
|---|---|
| 当前配置 `configuredMode` | 默认 `discount75`；保存手动规则后 `custom` |
| 响应脚本 | [g1-acq-discount.js](g1-acq-discount.js)，与 [g1-acq.js](g1-acq.js) 实现相同 |
| 响应运行版本 | `g1-acq-v7.9-mappings` |
| 报告脚本 | [g1-sku-report-discount.js](g1-sku-report-discount.js)，与 [g1-sku-report.js](g1-sku-report.js) 实现相同 |
| 报告版本 | `v7.9.0` |
| 默认 2TB 目标 | `g1.2tb.ai.12months_ip_90p`，无 `.acq` |
| 本地数据 | 原采集 / 执行两键 schema 1，新增独立映射键 `g1.sku.mappings.v1` |

首次安装时在小火箭模块管理中添加上方地址并启用。升级顺序为：**关闭旧模块 → 安装新地址 → 只开启 G1 ACQ SKU 管理一个改写入口 → 重新连接小火箭 → 重新打开 Google App 的方案页**。曾安装 **G1 ACQ Ultra 75p**、**G1 ACQ**、**G1 ACQ Ultra 一折**、**G1 ACQ Ultra 试用**、独立 **G1 SKU Scan**、**G1 Pro Annual Test** 或其它改写副本时，停用旧副本，避免重复计数或改写。

安装入口与全部内部脚本固定引用 `@v7.9.0`，无需单独添加脚本。**旧固定链接需换成新地址，不会自动更新。** 当前树保留四个模块，试用模块仍缺席；历史 `v7.8.0` / `v7.7.0` / `v7.6.0` / `v7.5.0` / `v7.4.0` / `v7.3.0` / `v7.2.0` / `v7.1.0` / `v7.0.0` 与 `pro-annual-test-v1` / `pro-annual-test-v2` 标签保持原样，旧试用链接仍指向原内容。

升级沿用 `g1.sku.scan.v1`、`g1.pro.annual.test.v1`，两份数据的 `schemaVersion=1` 保持不变；累计 SKU、次数、最近 10 次响应和旧执行记录继续保留。当前配置与实际采集、执行记录分别显示，保存或恢复规则不重写历史版本、模式、目标与时间。

[g1-acq-v7.sgmodule](g1-acq-v7.sgmodule) 保留名称 **G1 ACQ**；[g1-pro-annual-test.sgmodule](g1-pro-annual-test.sgmodule) 保留名称 **G1 Pro Annual Test** 和仅 `/ai?` 的响应范围，二者使用同一本地映射配置。[g1-sku-scan.sgmodule](g1-sku-scan.sgmodule) 仅更新下载 pin 和报告请求体配置，扫描实现仍为 `pro-annual-test-v2`，保持响应原文，没有模式或响应事件字段；报告保留其真实旧版本、空模式与缺失历史。四个报告 `http-request` 均设置 `requires-body=1,max-size=0`，供本地表单提交使用。

## 默认规则与手动映射

主入口先采集 `one.google.com/` 原始文字响应，再仅对 `one.google.com/ai?` 响应中的完整 SKU 执行规则。首次使用或恢复默认时启用以下三条：

| 匹配方式 | 源 | 目标 ID |
|---|---|---|
| 前缀 `prefix` | `g1.ios.2tb.` | `g1.2tb.ai.12months_ip_90p` |
| 精确 `exact` | `g1.ios.20tb.m.v2` | `g1.20tb.3months_ip_75p.acq` |
| 精确 `exact` | `g1.ios.30tb.ai.pro.m` | `g1.ai.ultra.3months_ip_75p.acq` |

编辑时选择**精确或前缀**，填写源与完整目标 ID，勾选“启用”后保存。精确源与目标支持 `g1.*` / `com.google.one.*`，命名空间之后有 1–160 个字母、数字、点、下划线或连字符，完整 ID 不以点结尾；**前缀源以点结尾**且命名空间之后有内容，只匹配带非空后缀的完整 SKU。相同匹配方式与源的规则重复、源与目标相同、非法 ID 或超过 **30 条**时提交被拒绝，禁用规则也计入上限与重复检查。

**启用的精确规则优先；否则采用最长的启用前缀。** 禁用规则不参与匹配；每个原始 ID 只替换一次，目标不再继续匹配另一条规则。删除全部规则或全部取消启用并保存后，响应原样通过。目标是否已在原响应或累计清单中出现，不影响规则执行；本版无备用目标，命中行 `fallback=false`。

默认 2TB 候选来自用户截图，目标没有 `.acq`；标签 `$1.99 × 12 months = $23.88/year` 表达连续 12 个月的月度优惠，`90p` 表示 **90% off、约一折**。截图标题 **Pro 5TB** 未改变默认 `g1.ios.2tb.` 源，也未新增 5TB 前缀；截图标签不等同于 iOS 商品、一次年付扣款或实际 5TB 权益。

## 安装（Shadowrocket / 小火箭）

进入 **配置页 → 当前使用配置右侧的 (i) 图标 → HTTPS 解密**。

![配置页入口示意：点击当前使用配置右侧的 (i) 图标进入配置编辑，才能看到 HTTPS 解密](docs-config-entry-example.png)

图中配置名为示例；个人文件列表、修改时间、文件大小和设备状态栏已移除。

1. 在小火箭模块管理中启用 `v7.9.0` **G1 ACQ SKU 管理**，只开启一个改写入口；旧版用户先关闭旧模块再安装新地址，首页保持 VPN 连接。
2. 从上述当前配置的 **(i)** 入口进入 **HTTPS 解密**，打开解密开关。已有可用且已受信任的 Shadowrocket 证书时直接沿用；尚未配置证书时，在该页生成并安装证书，再完成下两步。
3. 首次配置证书时，在 iOS **设置 → 通用 → VPN 与设备管理** 中完成 Shadowrocket 描述文件安装。
4. 首次配置证书时，在 iOS **设置 → 通用 → 关于本机 → 证书信任设置** 中开启对应 Shadowrocket 根证书的信任。
5. 在当前配置的 **HTTPS 解密 hostname 列表**中核对 `one.google.com`；模块通过 `%APPEND%` 追加该域名。这里是 HTTPS 解密列表，不是 DNS Hosts。
6. 安装、升级模块或更改解密配置后，重新连接小火箭，再重新进入 Google App 的方案页面。

## 手机端使用流程

1. 保持主入口开启与小火箭连接，在 **Google App（彩色 G 图标）→ Google One 管理** 浏览实际提供的 2TB / Pro、20TB / 30TB、月付、年付和优惠入口，让模块采集原始 SKU。
2. 在 Safari 打开[本地报告](https://one.google.com/g1-sku-report)，核对 **本机脚本已接管** 与 **v7.9.0**。从采集列表点源预填入口，或在新增规则表单中输入源；源与目标输入支持已采集 SKU 候选，也可填写完整 ID。
3. 选择精确或前缀、填写目标、勾选启用，再点该条的**保存**；编辑已有卡片使用同样步骤，取消勾选后保存可禁用。**删除**只移除该条；**恢复默认**还原上表三条，不清历史。
4. 保存成功后页面返回成功提示；刷新当前配置并核对规则。非法提交保留本次填写并显示错误；旧页面冲突提示后刷新、核对最新规则再提交；保存失败按页面提示处理，不把未成功保存的内容当作生效配置。
5. 重新进入 Google App 方案页，使下一次 `/ai?` 响应使用已保存配置；刷新报告，分别核对当前 `mappingConfig.revision` 与最近执行的 `test.mappingRevision`、`test.version`、`test.mode`、`test.updatedAt`。保存本身不改变旧执行记录。
6. 核对 App Store 实际商品名称、周期、初次与续费金额，再点 **导出 JSON** 保存本次记录；默认目标核对 2TB / Pro 和 Ultra，手动规则核对其实际源与目标。

![Google One Ultra 方案选择面板：旧模块的 20TB 方案入口示例](docs-ultra-plans-example.png)

截图展示 20TB 入口；2TB 12个月优惠测试应打开 **2TB / Pro** 方案。账号、地区和版本会影响页面展示，以手机实际提供的方案为准。原生 StoreKit 界面自身不属于网页响应改写范围。

浏览方案且收到匹配响应时，主模块先读取原始正文中的 `g1.*` 与 `com.google.one.*`，按 ID 去重累计次数，最多保留 **300 个 ID**，然后才执行符合 `/ai?` 范围的映射。重复 ID 更新次数和时间；到达容量后继续更新已保存 ID，并记录被容量丢弃的新 ID 数。采集保存改写前的原始编号，跳过本地报告页；手机保存候选 ID、次数、UTC 时间、扫描状态和最多 **10 条**脱敏响应事件，不保存完整响应正文，不查询 App Store，不发网络上报。

查找页面未展示的 SKU 时，依次浏览 Google App 实际提供的月付、年付和优惠入口：原始响应中携带的编号即使未显示在界面上，也会被采集。服务器未下发的编号不会进入原始清单；指定候选由模块替换后，在 App Store 原生订阅界面验证。完整 iOS 操作见 [SKU 采集与候选验证](docs/pro-annual-test.md#6-在-ios-小火箭中采集与验证隐藏-sku)。

## 本地报告与排错

保持主入口开启与小火箭连接，在 Safari 打开[本地报告](https://one.google.com/g1-sku-report)。页面显示 **Google One SKU 管理**、**本机脚本已接管** 与 **v7.9.0**，分别展示当前映射配置、实际采集和最近执行记录；操作区可[导出 JSON](https://one.google.com/g1-sku-report?format=json)。

页面由 Shadowrocket 的 `http-request` 脚本在手机本机生成，拦截 `/g1-sku-report`、末尾 `/` 与 query，直接返回 HTML 或 JSON。编辑使用原生 HTML POST 表单，无页面 JavaScript 或外部资源；成功后以 303 返回报告 GET，刷新不重复提交。规则变更只接受报告同源表单，普通 GET、导出 JSON 和 GET 的 `action` 参数均不修改映射。

若看到 Google 普通页面、404，或没有本机标识与编辑控件，核对新主入口、小火箭连接、**当前配置 (i) → HTTPS 解密**、`one.google.com` hostname 与受信任证书，再粘贴完整地址打开。

| 当前 JSON 字段 | 含义 |
|---|---|
| `reportVersion=v7.9.0` | 报告脚本版本，与保存的采集、执行版本独立 |
| `mappingConfig` | 当前有效配置：`status, revision, updatedAt, mode, customized, rules, target, targetOrigin, error, limit`，`limit=30` |
| `mappingConfig.rules[]` | `{id, match, from, to, enabled}`；规则 ID 由页面创建，`match` 为 `exact` 或 `prefix` |
| `mappingConfig.status` | `default` 默认、`custom` 手动、`invalid` 配置损坏、`unavailable` 读取失败；后两种暂停改写并显示错误 |
| `configuredMode` / `configuredTarget` | 从当前 `mappingConfig` 派生；默认 `discount75` / `g1.2tb.ai.12months_ip_90p` |
| `scan.version` / `scan.mode` | 实际采集版本与模式；本版版本为 `g1-acq-v7.9-mappings`，模式取执行当时配置 |
| `test.version` / `test.mode` | 最近执行版本与模式，保留到下一次执行记录成功写入 |
| `test.mappingRevision` / `test.mappingUpdatedAt` / `test.mappingStatus` | 最近执行使用的配置修订、配置保存 UTC 时间和状态；旧记录缺失时不补成当前配置 |
| `scan.updatedAt` / `test.updatedAt` | 实际采集 / 执行记录的 UTC 时间，与映射保存时间独立 |

`mappingConfig.revision` 是变更标识：未存储配置时为 `defaults-75p-v1`，成功保存生成新修订，用于拒绝旧页面覆盖新配置。配置读取出错时不假定默认规则；`invalid` 可用**恢复默认**修复，`unavailable` 需待本地存储恢复。非法输入显示 400，旧页面冲突显示 409，存储读取或保存失败显示 503；失败不显示保存成功，非法输入保留本次填写供修改。

| 保存的模式值 | 报告名称 |
|---|---|
| `discount75` | Ultra 三个月 75p（2.5折） |
| `custom` | 手动映射 |
| `trial` | Ultra 1个月免费试用优先（历史模式） |
| `discount` | Ultra 三个月一折优先（历史模式） |
| 空字符串、缺失或未知值 | 未记录 |

`scan.mode`、`test.mode` 只读取实际保存值，缺失或未知时 JSON 为空字符串、HTML 显示未记录，不按入口、旧版本名或目标补写。当前 `custom` 或 `discount75` 配置可与旧 `g1-acq-v7.8-75p` / `discount75`、`g1-acq-v7.7-trial` / `trial`、`g1-acq-v7.7-discount` / `discount` 记录并存；历史 `discount` 仍是一折模式。采集与执行独立写入两键，分别查看版本、模式、时间和执行配置修订。

只有 `g1-acq-v7.9-mappings` 标为本版采集记录；旧版本保留真实值与“旧采集记录”。已保存的最近 10 次响应继续显示；更早记录没有 `recentResponses` 时不补造历史，事件也不套用当前模式。版本未知或无记录时显示“尚未保存本版采集响应记录”。

| 采集字段 | 含义 |
|---|---|
| `scan.scannedResponses` | 累计处理的文字响应数，保持原有计数含义 |
| `scan.matchedResponses` | 文字响应中至少识别到一个候选 ID 的累计响应数，含重复 ID；保持原有含义 |
| `scan.nonTextResponses` | 累计非文本响应数，不计入 `scannedResponses` |
| `scan.recentResponses` | 最多 10 条响应事件；JSON 数组按接收顺序旧 → 新保存，HTML 表格按新 → 旧展示；每条含 `{at, path, bodyType, status, candidateCount, addedCount, omittedCount}` |

单次事件中，`candidateCount` 是该响应内去重后的候选数，`addedCount` 是本次新加入并实际保留的 ID 数，`omittedCount` 是本次因容量丢弃的不同 ID 数。状态包括 `new-skus`（新增）、`known-skus`（重复）、`no-skus`（无 ID）、`limit-reached`（无新增保留且有容量丢弃）、`non-text`（非文本）。`addedCount>0` 时优先标为 `new-skus`，即使同时 `omittedCount>0`；只有 `addedCount=0` 且 `omittedCount>0` 时才标为 `limit-reached`。路径只保留 `/`、`/ai`、`/plans`、`/_/…/data/batchexecute` 或 `[其他 one.google.com 路径]`，不保存 query、完整 URL、正文、请求头或响应头。

`configuredTarget` 与 `mappingConfig.target` 取当前启用且源恰为 `g1.ios.2tb.` 的前缀规则目标；删除或禁用该条后为空，单独的精确覆盖看执行行 `to`。默认 `targetOrigin=user-specified`，手动配置含该前缀时为 `user-configured`，缺少该前缀时为空。`test.target` / `test.targetOrigin` 保留执行当时的值；历史目标推断使用固定默认 2TB 常量，不用新配置回填旧记录：

| 历史执行记录 | 保留的 2TB 目标 | 保留的来源与模式 |
|---|---|---|
| `g1-acq-v7.8-75p` | `g1.2tb.ai.12months_ip_90p` | `user-specified`；`mode=discount75`、固定 75p 与 `fallback=false` 保留 |
| `g1-acq-v7.7-trial` | `g1.2tb.ai.12months_ip_90p` | `user-specified`；已保存的 `mode=trial` 保留试用历史含义 |
| `g1-acq-v7.7-discount` | `g1.2tb.ai.12months_ip_90p` | `user-specified`；已保存的 `mode=discount` 保留一折历史含义 |
| v7.5 / v7.6 | `g1.2tb.ai.12months_ip_90p` | `user-specified`；缺失 `mode` 时为空，版本保留 `g1-acq-v7.5` / `g1-acq-v7.6` |
| v7.3 / v7.4 | `g1.2tb.ai.annual_ip_90p.acq` | `user-specified`；缺失模式为空 |
| v7.2 | `g1.2tb.ai.annual.3months_ip_90p.acq` | `pattern-derived`；缺失模式为空 |
| v1 / v7.1 普通年付记录 | `g1.2tb.ai.annual` | `legacy`；缺失模式为空 |

`test.hitCount` 是全部规则的总改写数；`test.annualHitCount` 沿用兼容字段名，只统计源以 `g1.ios.2tb.` 开头的改写，手动精确规则命中此类源也计入。日志 `annualHits` 含义相同，报告显示“其中 2TB”；字段名不证明年付扣款。

`test.matchedFrom` 每行保留 `{id, to, count, fallback}`，`to` 为该源本次实际目标，可为 `g1.*` 或 `com.google.one.*`。本版 `fallback=false`，HTML 显示“否”。历史 `fallback=true` 保留当时使用备用的含义：v7.7 `discount` 备用为 75p，v7.7 `trial` 与 v7.6 备用为 90p，更早的 75p 行保持原值；HTML 按实际 `to` 显示 90p、75p 或“是”，目标不足时仅显示“是”。相同 75p 目标需结合版本、模式与 `fallback` 判断。

`status=rewritten` 表示至少一条规则命中，`no-match` 表示没有启用规则命中，`non-text` 表示非文本，`config-error` 表示配置损坏或读取失败并暂停改写；后三种原样返回响应。报告保存最近一次 `/ai?` 执行，后续零命中也会更新它。旧 v1 仅 2TB 行缺少 `to` 时补为 `g1.2tb.ai.annual`，缺少 `annualHitCount` 时从旧行次数计算，旧目标、来源、版本与已保存模式保持原值。

当前日志前缀为 `[g1-acq]`，运行版本为 `g1-acq-v7.9-mappings`，包含执行模式、映射状态与修订；采集摘要保留 `status`、`addedCount`、`candidates`、`omittedCount`、`occurrences`、`saved=yes/no`，改写摘要保留 `hits` 与 `annualHits`，命中行显示 `源 SKU → 实际目标 xN`。`saved=no` / `SKU collection unavailable` 表示本次采集未成功保存或未完成，报告仍可显示旧计数与时间。本版没有备用日志；历史 `fallback=实际完整目标编号` 与更早 `fallback=75p` 按原含义读取。

清单仍是旧 SKU 时，按下面顺序比较一次实际活动：

1. 在报告记录 `reportVersion`、`configuredMode`、实际 `scan.version` / `scan.mode`、`test.version` / `test.mode`、三个响应计数与最新事件时间，保留当前清单。
2. 从 Google App 重新进入实际提供的 2TB / Pro 方案，再浏览其它实际方案页，等待加载完成。
3. 返回 Safari 刷新报告。文字计数增长且最新事件为 `known-skus`，说明收到的是已保存 ID，次数会增加而清单不增加；`no-skus` 表示该响应未携带可识别 ID。若只增加 `nonTextResponses`，查看 `non-text` 事件。
4. 三个计数与最新事件时间均未变化，只能确认这轮未保存新的采集记录；存储写入失败也会保留旧计数。先查看脚本日志 `saved=no` / `SKU collection unavailable`，再逐项核对页面缓存、`one.google.com/` 匹配范围、VPN 连接、唯一开启的改写入口、当前配置的 HTTPS 解密、证书信任与 hostname。

重装或清空记录不会使服务器返回新编号。`?clear=1` 只清采集 / 测试两键，**不清映射**；**恢复默认只改映射，不清历史**。完整字段、表单反馈与实机记录模板见 [SKU 管理使用文档](docs/pro-annual-test.md)。

## 工作范围与本地存储

| 工具 | 脚本匹配范围 | 行为 |
|---|---|---|
| 主入口与 G1 ACQ 被动采集 | `^https://one\.google\.com/` 响应，跳过本地报告页 | 原始 SKU 采集、非文本计数、最近 10 次响应诊断 |
| 映射改写 | `^https://one\.google\.com/ai\?` 响应 | 采集后按当前配置单次替换，保存最近执行记录 |
| G1 Pro Annual Test 兼容入口 | `^https://one\.google\.com/ai\?` 响应 | 在原范围内使用同一映射配置 |
| 本地结果页 | `/g1-sku-report`、末尾 `/` 与 query | 读取配置和历史、导出 JSON；原生 POST 表单保存映射 |

| 本地键 | 内容 |
|---|---|
| `g1.sku.scan.v1` | 原 schema 1 采集数据，最多 300 个 ID、10 条脱敏响应事件 |
| `g1.pro.annual.test.v1` | 原 schema 1 最近执行数据，新增执行所用配置修订、时间与状态 |
| `g1.sku.mappings.v1` | 独立 schema 1 映射记录：`schemaVersion, revision, updatedAt, customized, rules`；最多 30 条 |

时间均使用 UTC。映射键不存在或为空时采用默认规则；显式保存空规则列表表示关闭改写，仍为 `custom`。映射键 JSON / schema / 规则损坏或读取失败时暂停改写，显示配置错误；普通报告 GET 与导出不写入配置。恢复默认保存 `customized=false`，恢复默认模式；采集与执行仍各自独立写入原两键。

- **脚本读取**：响应脚本采集原始正文，不读请求头或请求体；报告 POST 只读取 `Origin`、`Content-Type`、可选 `Sec-Fetch-Site` 与表单正文，不读 Cookie / 认证头。采集事件与导出不保存完整 URL、query、正文或请求头。
- **本地处理**：采集、改写、报告与映射共用手机 `$persistentStore`，无网络上报；HTML 无 JavaScript 和外部资源。模块与脚本下载使用固定 `@v7.9.0` GitHub / jsDelivr 地址。
- **改写范围**：只修改符合规则的商品 ID，不改价格字段；原生 StoreKit 返回另按实际收银台记录。
- **设备配置**：沿用 `[MITM]` 的 `%APPEND% one.google.com` 与 `[Rule]` 的全局 UDP 443 拦截；支持回退的 QUIC 请求转走 TCP，也会影响其它 App 的 HTTP/3、速度或连通性。脚本未申请定位、相册或通知权限，VPN、描述文件与根证书由设备配置管理。

## 文件

| 文件 | 用途 |
|---|---|
| [g1-acq-ultra-discount.sgmodule](g1-acq-ultra-discount.sgmodule) | G1 ACQ SKU 管理主入口，保留原文件名 |
| [g1-acq-v7.sgmodule](g1-acq-v7.sgmodule) | 保留名称 G1 ACQ 的兼容入口 |
| [g1-acq.js](g1-acq.js) | 响应唯一源，运行版本 `g1-acq-v7.9-mappings` |
| [g1-acq-discount.js](g1-acq-discount.js) | 自动生成、与响应源实现相同的兼容副本 |
| [g1-pro-annual-test.sgmodule](g1-pro-annual-test.sgmodule) | 保留名称与 `/ai?` 范围的兼容入口 |
| [g1-sku-scan.sgmodule](g1-sku-scan.sgmodule) | 旧版独立扫描模块，仅更新 pin 与报告请求体配置 |
| [g1-sku-scan.js](g1-sku-scan.js) | 历史独立扫描实现 `pro-annual-test-v2`，无模式与响应事件 |
| [g1-sku-report.js](g1-sku-report.js) | 本地管理页、POST 表单与 JSON 报告唯一源，`reportVersion=v7.9.0` |
| [g1-sku-report-discount.js](g1-sku-report-discount.js) | 自动生成、与报告源实现相同的兼容副本 |
| [lib/mapping-config.js](lib/mapping-config.js) | 响应与报告共享的映射配置引擎 |
| [tools/build-variants.cjs](tools/build-variants.cjs) | 嵌入共享配置引擎、生成两个兼容副本并检查同步 |
| [docs/pro-annual-test.md](docs/pro-annual-test.md) | 安装升级、手动映射、报告字段、历史兼容与记录模板 |

## 离线验证

使用 Node.js 18 或更新版本，在仓库根目录执行：

```sh
node tools/build-variants.cjs --check
node --test tests/*.test.cjs
```

[lib/mapping-config.js](lib/mapping-config.js) 是共享配置引擎源；[tools/build-variants.cjs](tools/build-variants.cjs) 更新 [g1-acq.js](g1-acq.js) 和 [g1-sku-report.js](g1-sku-report.js) 中的生成区块，再生成 [g1-acq-discount.js](g1-acq-discount.js) 与 [g1-sku-report-discount.js](g1-sku-report-discount.js)。修改源后运行 `node tools/build-variants.cjs`；`--check` 只检查两个源内区块和两个副本，不写文件。

验证项包括默认三条规则、手动增删改与恢复、禁用 / 空列表、精确优先 / 最长前缀 / 单次替换、30 条上限、表单校验与旧页面冲突、存储读取失败暂停改写、下一响应生效、历史记录独立、300 / 10 上限、JSON 与四个模块配置。结果以实际命令输出为准；离线验证字符串与报告行为，实机结果按文档模板填写。

## 贡献致谢

感谢 [@cehomecambodia-lang](https://github.com/cehomecambodia-lang) 通过 [PR #3](https://github.com/crhywun/g1-acq/pull/3) 提供 20TB / 30TB Ultra 1个月免费试用 SKU 映射和三个月 90p 备用方案。
