# google-ultra

简体中文 | [English](README.en.md)

Google One 的 AI Ultra 有个 12 个月免费试用的 offer，任何页面上都看不到，只存在于 Play 结算库（BillingClient）的返回值里。这个脚本把「查 offer → 拉起 Play 收银台 → 确认 0 元 → 开通」串成了一条命令。

原理是借 Google One 自己的进程调 BillingClient，不需要 Xposed，不需要改 APK。

## 两个版本

**简单版**：一个网页，双击 `web/index.html` 打开，登录 → 绑卡 → 点一下，完事。不用装 Python，不用 root，不用数据线。

**脚本版**：`ultra0.py`，要一台 root 手机和 frida。优点是 offer 从真实客户端取，不依赖浏览器指纹，能批量跑。

先试简单版，不行再上脚本。

## 能拿到什么

拿到的是 **Ultra 5x，20TB，12 个月 $0**，到期前取消就不用付钱。开通后 Play 订阅页会写「即将扣款 2027-11-17 US$99.99/月」，这是试用结束后的价格，不是现在扣。

30TB 那档（20x）也查得到 offer，但 Play 结算时会二次校验，大部分号会弹「您不符合免费试用条件」。这个脚本不会去点它。

## 需要什么

一台 root 过的安卓手机，USB 连着电脑。Magisk、KernelSU、APatch 都行，模拟器也行。

电脑上要 Python 3.9 以上，外加两个包：

```bash
pip3 install frida
```

adb 用 Android SDK 里的 platform-tools。装好后 `adb devices` 能看到手机就行。没装过的话，Mac 上 `brew install android-platform-tools`，Windows 去下 platform-tools 压缩包，解压后把目录加进 PATH。

手机上要开「开发者选项 → USB 调试」，插上后在弹窗里点允许。

## 用法

### 简单版怎么用

源码在 `web/index.html`，单文件，下载后双击就能用，浏览器会打开一个页面：

1. 点「去登录」，用要开的谷歌账号登进去
2. 点「去绑卡」，绑一张卡，卡里留 1 美元就行，试用期间不扣
3. 回页面，点「Ultra 20 TB」那张卡片

跳到 Google 的结算页之后，确认金额是 0，点订阅即可。

不想下仓库的话，也可以直接访问在线版：https://gylive.github.io/google-ultra/web/

### 脚本版怎么用

先自检一遍环境，它会检查设备、root、frida-server、推送计费模块，还会顺手关掉会拦截 Play 网络的代理和 Clash：

```bash
python3 ultra0.py --check
```

全绿就可以跑号了。

账号写成一个文本文件，一行一个，字段用四个短横线隔开：

```
邮箱----密码----辅助邮箱----2FA密钥----卡号----有效期----CVC----持卡人----邮编
```

后面几项留空也行。刚买的新号一般没有 2FA，那就留空：

```
abc123@gmail.com----mypassword----
```

带卡的话：

```
abc123@gmail.com----mypassword--------4242424242424242----10/29----326----ZHANG SAN----97201
```

地址不填会默认用 `123 Main St, Portland, OR 97201`，俄勒冈免消费税。

跑：

```bash
python3 ultra0.py accounts.txt
```

单个号也可以：

```bash
python3 ultra0.py --account "abc@gmail.com:密码:2FA密钥"
```

想看结果，开订阅页确认一下：

```
Play 商店 → 头像 → 付款和订阅 → 订阅
```

## 它内部干了什么

1. 把账号加到手机（没有就加，有就跳过）
2. 切 Play 账号。切完一定从头像面板读回来核对，对不上直接退出，不会跑到别的号上
3. 启动 Google One，用 Frida 挂进去，`DexClassLoader` 加载计费模块
4. `queryProductDetailsAsync` 查 `g1.play.20tb`，拿回 5 个 offer
5. 挑免费期最长的那个（`eft-12month`，P1Y），在主线程调 `launchBillingFlow`
6. 盯着收银台。看到价格是 0 或者「免费试用 12 个月」才点订阅；看到「不符合免费试用条件」就关掉，绝不点
7. Play 有时会要求重新输密码，脚本自己填
8. 打开订阅页核对，结果写进 `ultra0-report/`

## 失败的时候是什么情况

**账号不符合资格。** Play 会说「您不符合免费试用条件，尽管如此，您仍能订阅」。一个号基本只有一次机会：试用一旦发起过，哪怕没确认，offer 也会消失。所以别拿主力号试。

**没有付款方式。** 订阅必须有卡。Play 的加卡页是 WebView，adb 填不进去，脚本会停下来提示你在手机上填完再按回车。每台设备手动绑一次，之后所有号共用这张卡。

**Play 账号没切过来。** 脚本会打印当前实际登录的是哪个号，然后退出。这时手动在 Play 里切一下，加上 `--no-switch` 重跑，让它只做校验不再尝试切换：

```bash
python3 ultra0.py accounts.txt --no-switch
```

**收银台没弹出来。** 重跑一次。连续两次都不弹，检查一下设备的网络：`settings get global http_proxy`，不是 `null` 就清掉；Clash 这类 VPN 会劫持 DNS，也会让 Play 连不上。

## 一些参数

```bash
--plan 5x      # 只开 20TB
--plan 20x     # 只查 30TB（多数号拿不到试用）
--plan auto    # 默认，两档都查，挑免费期最长的
--no-add       # 账号已经在手机上，跳过加号
--no-switch    # 不自动切账号，只校验当前账号
```

## 几个说明

试完之后记得去 Play 里取消订阅，不然到期会扣 $99.99。

脚本只在本地跑，账号密码和卡号不联网，也不上传任何地方。报告文件在 `ultra0-report/`，里面有 offer 明细和每步结果。

这段代码只对 Play 结算流程里「客户端能拿到试用 offer」这件事做了利用，没有改任何服务端数据。

## 交流

有问题进群问。六个群随便进一个，都满了就加我微信，备注「Ultra」，我拉你。

<table>
<tr>
<td align="center"><b>2 群</b><br><img src="assets/wechat-group.jpg" width="240" alt="2 群"></td>
<td align="center"><b>3 群</b><br><img src="assets/wechat-group3.jpg" width="240" alt="3 群"></td>
<td align="center"><b>4 群</b><br><img src="assets/wechat-group4.jpg" width="240" alt="4 群"></td>
</tr>
<tr>
<td align="center"><b>5 群</b><br><img src="assets/wechat-group5.jpg" width="240" alt="5 群"></td>
<td align="center"><b>6 群</b><br><img src="assets/wechat-group6.jpg" width="240" alt="6 群"></td>
<td align="center"><b>都满了加我</b><br><img src="assets/wechat-admin.jpg" width="240" alt="微信"></td>
</tr>
</table>

## 友链

本项目在 [LINUX DO](https://linux.do) 社区发布与讨论，感谢社区的支持。

- [LINUX DO](https://linux.do) —— 新的理想主义者社区
- [LINUX DO 开源推广](https://linux.do/tag/1014) —— 开源项目推广板块

[![LINUX DO](https://img.shields.io/badge/LINUX%20DO-%E7%A4%BE%E5%8C%BA-1f6feb)](https://linux.do)

## 仓库结构

```
web/index.html      简单版：单文件网页，在线版就是它
simple/ultra.py     脚本版极简实现（带内嵌模块）
simple/README.md    脚本版教程
ultra0.py           脚本版完整实现（批量、选档、切号）
assets/             群二维码
```

在线版地址：https://gylive.github.io/google-ultra/web/

## 许可

MIT，见 [LICENSE](LICENSE)。
