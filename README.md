# 反向代理服务器

高性能的 Http 反向代理服务器，应用的场景是，多个接口或站点在同一台服务器上，且都要通过同一个端口返回数据。

# 功能

[ √ ] 根据配置文件，自动反向代理域名到目标地址

[ √ ] 支持免费Https

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
| hosts.ip   |  是  ||  目标IP  |


## https

使用以下配置
```yaml
port: 80
ssl: true
ssl_port: 443
ssl_key_file: './ssl/certificate.crt'
ssl_cert_file: './ssl/private.pem'
hosts:
  "l.j-k.one":
    port: 81
    ip: "127.0.0.1"
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