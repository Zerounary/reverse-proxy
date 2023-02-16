# 反向代理服务器

高性能的 Http 反向代理服务器，应用的场景是，多个接口或站点在同一台服务器上，且都要通过同一个端口返回数据。

# 功能

[ √ ] 根据配置文件，自动反向代理域名到目标地址

[ √ ] 支持免费Https，并自动续期

[ × ] 支持 DNS 接口，自动绑定域名IP

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
    protocol: "http"
```
`port`是反向代理服务器的端口，`hosts`记录个每个域名所代理的内网环境。

每一个`host`有以下结构

`ip`：目标IP
`port`：目标端口
`protocol`：代理的协议

## https

使用以下配置
```yaml
port: 80
ssl: true
ssl_port: 443
ssl_key_file: './ssl/key.pem'
ssl_cert_file: './ssl/cert.pem'
hosts:
  "l.j-k.one":
    port: 81
    ip: "127.0.0.1"
    protocol: "http"

```