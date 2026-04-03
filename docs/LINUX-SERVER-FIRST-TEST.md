# Linux Server First-Test Checklist / Linux 服务器首测清单

## English

This checklist is the shortest safe path for validating `Rivulet Gateway` on a Linux server.

Current release note:

- use the current GitHub Release tag, for example `v0.1.6`
- package filenames should match the same release version

If the release tag version and asset filename version ever diverge, treat that as a release issue and stop the server test until the assets are corrected.

### 0. Common Precheck

Detect the server architecture:

```bash
uname -m
```

Typical outputs:

- `x86_64`
- `aarch64`

Install minimum tools:

```bash
sudo apt-get update
sudo apt-get install -y curl tar sha256sum python3
```

For RPM validation on RPM-based systems:

```bash
sudo yum install -y rpm
```

Or on Debian/Ubuntu when only payload inspection is needed:

```bash
sudo apt-get install -y rpm cpio
```

Create a clean test directory:

```bash
mkdir -p ~/rivulet-first-test
cd ~/rivulet-first-test
```

### 1. x86_64 Tarball First Test

Download assets:

```bash
curl -L -o rivulet-gateway-<version>-linux-x86_64.tar.gz \
  https://github.com/hjd92215202/Rivulet-Gateway/releases/download/v<version>/rivulet-gateway-<version>-linux-x86_64.tar.gz

curl -L -o SHA256SUMS.txt \
  https://github.com/hjd92215202/Rivulet-Gateway/releases/download/v<version>/SHA256SUMS.txt
```

Verify checksum:

```bash
grep 'rivulet-gateway-<version>-linux-x86_64.tar.gz' SHA256SUMS.txt | sha256sum -c -
```

Start a local backend:

```bash
mkdir -p ~/rivulet-backend
cd ~/rivulet-backend
python3 -m http.server 9000 --bind 127.0.0.1
```

Open a second terminal and start the gateway:

```bash
cd ~/rivulet-first-test
rm -rf ./x86_64-test
mkdir -p ./x86_64-test
tar -xzf rivulet-gateway-<version>-linux-x86_64.tar.gz -C ./x86_64-test
cd ./x86_64-test/rivulet-gateway-<version>
./usr/bin/gateway ./etc/gateway/gateway.toml
```

Open a third terminal and validate behavior:

```bash
curl -i -H 'Host: localhost' http://127.0.0.1:8080/
curl -i -H 'Host: wrong.test' http://127.0.0.1:8080/
ss -lntp | grep 8080
```

Expected result:

- `Host: localhost` should return `200` when the backend is alive
- `Host: wrong.test` should return `404`
- if the backend is stopped, `Host: localhost` should return `502`

### 2. arm64 Tarball First Test

Download assets:

```bash
curl -L -o rivulet-gateway-<version>-linux-arm64.tar.gz \
  https://github.com/hjd92215202/Rivulet-Gateway/releases/download/v<version>/rivulet-gateway-<version>-linux-arm64.tar.gz

curl -L -o SHA256SUMS.txt \
  https://github.com/hjd92215202/Rivulet-Gateway/releases/download/v<version>/SHA256SUMS.txt
```

Verify checksum:

```bash
grep 'rivulet-gateway-<version>-linux-arm64.tar.gz' SHA256SUMS.txt | sha256sum -c -
```

Start a local backend:

```bash
mkdir -p ~/rivulet-backend
cd ~/rivulet-backend
python3 -m http.server 9000 --bind 127.0.0.1
```

Open a second terminal and start the gateway:

```bash
cd ~/rivulet-first-test
rm -rf ./arm64-test
mkdir -p ./arm64-test
tar -xzf rivulet-gateway-<version>-linux-arm64.tar.gz -C ./arm64-test
cd ./arm64-test/rivulet-gateway-<version>
./usr/bin/gateway ./etc/gateway/gateway.toml
```

Open a third terminal and validate behavior:

```bash
curl -i -H 'Host: localhost' http://127.0.0.1:8080/
curl -i -H 'Host: wrong.test' http://127.0.0.1:8080/
ss -lntp | grep 8080
```

Expected result is the same as x86_64.

### 3. Optional RPM Validation

x86_64 RPM:

```bash
curl -L -o rivulet-gateway-<version>-1.x86_64.rpm \
  https://github.com/hjd92215202/Rivulet-Gateway/releases/download/v<version>/rivulet-gateway-<version>-1.x86_64.rpm

grep 'rivulet-gateway-<version>-1.x86_64.rpm' SHA256SUMS.txt | sha256sum -c -
```

arm64 RPM:

```bash
curl -L -o rivulet-gateway-<version>-1.aarch64.rpm \
  https://github.com/hjd92215202/Rivulet-Gateway/releases/download/v<version>/rivulet-gateway-<version>-1.aarch64.rpm

grep 'rivulet-gateway-<version>-1.aarch64.rpm' SHA256SUMS.txt | sha256sum -c -
```

If you cloned the repository onto the server, you can use the built-in validation suite:

```bash
bash ./packaging/tests/run-linux-validation.sh /path/to/rivulet-gateway-<version>-1.x86_64.rpm
bash ./packaging/tests/run-linux-validation.sh /path/to/rivulet-gateway-<version>-1.aarch64.rpm
```

### 4. Optional systemd Smoke

Repository-owned automated path:

```bash
bash ./scripts/linux-service-install.sh --tag v<version> --format tar.gz
```

Then inspect service state:

```bash
sudo systemctl status rivulet-gateway --no-pager
```

Inspect logs:

```bash
journalctl -u rivulet-gateway -n 200 --no-pager
```

Validate traffic:

```bash
curl -i -H 'Host: localhost' http://127.0.0.1:8080/
```

If you want the repository to remove the installed service again:

```bash
bash ./scripts/linux-service-remove.sh --mode auto
```

### 5. Suggested First-Test Exit Criteria

- the gateway listens on `0.0.0.0:8080`
- a valid request with `Host: localhost` returns `200`
- an invalid host returns `404`
- a stopped backend produces `502`
- the process can be started and stopped cleanly
- if systemd is tested, `systemctl restart` also succeeds

## 中文

这份清单给出在 Linux 服务器上验证 `Rivulet Gateway / 溪流网关` 的最短可控路径。

当前发布说明：

- 使用当前 GitHub Release tag，例如 `v0.1.6`
- 包文件名应当与同一 release 版本保持一致

如果 release tag 版本与资产文件名版本发生偏差，应当先视为发布问题并停止服务器首测，先修正发布资产再继续。

### 0. 通用前置检查

先确认服务器架构：

```bash
uname -m
```

常见输出：

- `x86_64`
- `aarch64`

安装最小工具：

```bash
sudo apt-get update
sudo apt-get install -y curl tar sha256sum python3
```

如果要验证 RPM：

RPM 系发行版可以执行：

```bash
sudo yum install -y rpm
```

Debian / Ubuntu 如果只是做 payload 检查，可以执行：

```bash
sudo apt-get install -y rpm cpio
```

准备干净测试目录：

```bash
mkdir -p ~/rivulet-first-test
cd ~/rivulet-first-test
```

### 1. x86_64 Tarball 首测

下载发布资产：

```bash
curl -L -o rivulet-gateway-<version>-linux-x86_64.tar.gz \
  https://github.com/hjd92215202/Rivulet-Gateway/releases/download/v<version>/rivulet-gateway-<version>-linux-x86_64.tar.gz

curl -L -o SHA256SUMS.txt \
  https://github.com/hjd92215202/Rivulet-Gateway/releases/download/v<version>/SHA256SUMS.txt
```

校验 checksum：

```bash
grep 'rivulet-gateway-<version>-linux-x86_64.tar.gz' SHA256SUMS.txt | sha256sum -c -
```

启动本地后端：

```bash
mkdir -p ~/rivulet-backend
cd ~/rivulet-backend
python3 -m http.server 9000 --bind 127.0.0.1
```

打开第二个终端，启动网关：

```bash
cd ~/rivulet-first-test
rm -rf ./x86_64-test
mkdir -p ./x86_64-test
tar -xzf rivulet-gateway-<version>-linux-x86_64.tar.gz -C ./x86_64-test
cd ./x86_64-test/rivulet-gateway-<version>
./usr/bin/gateway ./etc/gateway/gateway.toml
```

打开第三个终端，验证行为：

```bash
curl -i -H 'Host: localhost' http://127.0.0.1:8080/
curl -i -H 'Host: wrong.test' http://127.0.0.1:8080/
ss -lntp | grep 8080
```

预期结果：

- `Host: localhost` 在后端正常时应返回 `200`
- `Host: wrong.test` 应返回 `404`
- 后端停止后，`Host: localhost` 应返回 `502`

### 2. arm64 Tarball 首测

下载发布资产：

```bash
curl -L -o rivulet-gateway-<version>-linux-arm64.tar.gz \
  https://github.com/hjd92215202/Rivulet-Gateway/releases/download/v<version>/rivulet-gateway-<version>-linux-arm64.tar.gz

curl -L -o SHA256SUMS.txt \
  https://github.com/hjd92215202/Rivulet-Gateway/releases/download/v<version>/SHA256SUMS.txt
```

校验 checksum：

```bash
grep 'rivulet-gateway-<version>-linux-arm64.tar.gz' SHA256SUMS.txt | sha256sum -c -
```

启动本地后端：

```bash
mkdir -p ~/rivulet-backend
cd ~/rivulet-backend
python3 -m http.server 9000 --bind 127.0.0.1
```

打开第二个终端，启动网关：

```bash
cd ~/rivulet-first-test
rm -rf ./arm64-test
mkdir -p ./arm64-test
tar -xzf rivulet-gateway-<version>-linux-arm64.tar.gz -C ./arm64-test
cd ./arm64-test/rivulet-gateway-<version>
./usr/bin/gateway ./etc/gateway/gateway.toml
```

打开第三个终端，验证行为：

```bash
curl -i -H 'Host: localhost' http://127.0.0.1:8080/
curl -i -H 'Host: wrong.test' http://127.0.0.1:8080/
ss -lntp | grep 8080
```

预期结果与 x86_64 相同。

### 3. 可选 RPM 验证

x86_64 RPM：

```bash
curl -L -o rivulet-gateway-<version>-1.x86_64.rpm \
  https://github.com/hjd92215202/Rivulet-Gateway/releases/download/v<version>/rivulet-gateway-<version>-1.x86_64.rpm

grep 'rivulet-gateway-<version>-1.x86_64.rpm' SHA256SUMS.txt | sha256sum -c -
```

arm64 RPM：

```bash
curl -L -o rivulet-gateway-<version>-1.aarch64.rpm \
  https://github.com/hjd92215202/Rivulet-Gateway/releases/download/v<version>/rivulet-gateway-<version>-1.aarch64.rpm

grep 'rivulet-gateway-<version>-1.aarch64.rpm' SHA256SUMS.txt | sha256sum -c -
```

如果服务器上已经 clone 了仓库，可以直接用仓库内校验脚本：

```bash
bash ./packaging/tests/run-linux-validation.sh /path/to/rivulet-gateway-<version>-1.x86_64.rpm
bash ./packaging/tests/run-linux-validation.sh /path/to/rivulet-gateway-<version>-1.aarch64.rpm
```

### 4. 可选 systemd Smoke

复制文件到系统路径：

```bash
sudo cp ./usr/bin/gateway /usr/bin/gateway
sudo mkdir -p /etc/gateway
sudo cp ./etc/gateway/gateway.toml /etc/gateway/gateway.toml
sudo cp ./usr/lib/systemd/system/rivulet-gateway.service /etc/systemd/system/rivulet-gateway.service
```

启用并启动：

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now rivulet-gateway
sudo systemctl status rivulet-gateway --no-pager
```

查看日志：

```bash
journalctl -u rivulet-gateway -n 200 --no-pager
```

验证流量：

```bash
curl -i -H 'Host: localhost' http://127.0.0.1:8080/
```

### 5. 建议的首测通过标准

- 网关能监听 `0.0.0.0:8080`
- 带 `Host: localhost` 的有效请求返回 `200`
- 错误 host 返回 `404`
- 停掉后端后返回 `502`
- 进程可以正常启动和停止
- 如果测了 systemd，则 `systemctl restart` 也能成功
