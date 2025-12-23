# 反向代理服务器

高性能的 Http 反向代理服务器，应用的场景是，多个接口或站点在同一台服务器上，且都要通过同一个端口返回数据。

# 功能

[ √ ] 根据配置文件，自动反向代理域名到目标地址

[ √ ] 支持 HTTP/HTTPS 热更新（配置或证书改动 1s 内自动生效）

[ √ ] 支持共享 80/443 端口，按 Host/SNI 分发到不同后端

[ √ ] 支持全局证书 + 每个 Host 独立证书（SNI）

[ × ] 支持 DNS 接口，自动绑定域名IP

[ × ] 自动申请和续期https证书

[ × ] 支持负载均衡策略


# 性能
|指标| Nginx | RP | 原服务|
| ---   | ---   | --- | --- |
| QPS   |  5985  |  50243  | 101088 |
| 平均延迟   |  1707ms  |  9.46ms  | 4.45ms |
| 平均流量   |  1.2MB  |  7.79MB  | 15.7MB |

# 使用

首先，需要一个配置yaml文件
```yaml
port: 80
hosts:
  "l.j-k.one":
    port: 81
    ip: "127.0.0.1"
```
`port`是反向代理服务器的端口，`hosts`记录个每个域名所代理的内网环境。

|字段| 必填 | 默认值 | 说明 |
| ---   | ---  | ---     | --- |
| port   |  否  | 80|  HTTP反向代理的端口  |
| hosts   |  否  ||  反向代理的域名详情  |
| hosts.port   |  是  ||  目标端口  |
| hosts.ip   |  是  ||  目标IP或者域名  |
| hosts.protocol   |  是  ||  目标的协议，支持 http/https  |


## https

使用以下配置
```yaml
port: 80
ssl: true
ssl_port: 443
ssl_key_file: "./ssl/private.pem"
ssl_cert_file: "./ssl/certificate.crt"
hosts:
  "l.j-k.one":
    port: 81
    ip: "127.0.0.1"
    protocol: "http"
```

|字段| 必填 | 默认值 | 说明 |
| ---   | ---  | ---     | --- |
| ssl   |  否  | false|  是否启用https  |
| ssl_port   |  否  |443|  https端口  |
| ssl_key_file   |  否  | ./ssl/private.pem|  证书私钥  |
| ssl_cert_file   |  否  | ./ssl/certificate.crt|  证书certificate  |

推荐几个免费的https证书申请地址[freessl](https://freessl.cn/)、[osfipin](https://letsencrypt.osfipin.com/)

使用 `*.j-k.one` 泛域名的形式申请证书

下载证书后，将`certificate.crt`、`private.pem`复制到ssl目录下即可

### 每个 Host 独立证书（SNI）

如果你需要不同域名使用不同证书，可在 `hosts` 条目下注明 `tls` 字段，示例如下：

```yaml
ssl: true
ssl_port: 443
# 可选：全局兜底证书
# ssl_key_file: "./ssl/default.key.pem"
# ssl_cert_file: "./ssl/default.cert.pem"
hosts:
  "api.example.com":
    port: 9000
    ip: "127.0.0.1"
    protocol: "http"
    tls:
      cert_file: "./ssl/api.cert.pem"
      key_file: "./ssl/api.key.pem"
  "admin.example.com":
    port: 9100
    ip: "127.0.0.1"
    protocol: "http"
    tls:
      cert_file: "./ssl/admin.cert.pem"
      key_file: "./ssl/admin.key.pem"
```

注意事项：

1. `tls.cert_file` 与 `tls.key_file` 需成对出现，缺失则回退到全局证书。
2. 所有证书/私钥改动、或 `config.yml` 更新，都由内置 watcher 检测并在 1 秒内触发热重启，无需手动重启服务。
3. 如果未提供全局证书，程序会自动使用任意一个 Host 的证书作为兜底。

## 阿里云获取

如果是使用 阿里云 获取的免费证书，下载 Nginx （pem/key）的格式的证书 复制到ssl目录后参考以下配置

```yaml
ssl: true
ssl_port: 443
ssl_key_file: './ssl/l.j-k.one.key'
ssl_cert_file: './ssl/l.j-k.one.pem'
hosts:
  "l.j-k.one":
    port: 90
    ip: "127.0.0.1"
    protocol: "http"
```

但是阿里云的免费域名不支持泛域名

## porkbun获取

porkbun 可以免费生成的ssl证书，但前提是购买了他家的域名。

进入域名管理，点开对应的域名详情，下面会有一个SSL，点击SSL旁边的编辑图标跳转拿到生成页面，点击revoke

等待十几分钟后，回到页面就可以下载证书了。

下载后使用类似下类配置

```yaml
ssl: true
ssl_port: 443
ssl_key_file: './ssl/private.key.pem'
ssl_cert_file: './ssl/domain.cert.pem'
hosts:
  "l.j-k.one":
    port: 90
    ip: "127.0.0.1"
    protocol: "http"

```