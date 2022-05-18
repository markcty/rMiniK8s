# minik-8-s/rminik8s

## Getting Started

Download links:

SSH clone URL: ssh://git@git.jetbrains.space/minik8s/minik-8-s/rminik8s.git

HTTPS clone URL: <https://git.jetbrains.space/minik8s/minik-8-s/rminik8s.git>

These instructions will get you a copy of the project up and running on your local machine for development and testing
purposes.

## Prerequisites

What things you need to install the software and how to install them.

```
docker
```

## Deployment

### Master

```shell
# please run in root
bash <(curl -s https://s3.jcloud.sjtu.edu.cn/1b088ff214b04e6291c549a95685610b-share/deploy-master.sh)
```

### Node

在部署node之前需要保证ETCD中写入以下规则，并且启用v2 API（在ETCD启动参数中加入–enable-v2）：

```shell
ETCDCTL_API=2 etcdctl set /coreos.com/network/config '{ "Network": "10.5.0.0/16", "Backend": {"Type": "vxlan"}}'
```

建议使用multipass起虚拟机来测试，启动带docker的multipass：

```shell
multipass launch docker -n k8s1
multipass shell k8s1
```

在虚拟机中运行以下命令部署node，其间会要求输入配置：

```shell
# please run in root
bash <(curl -s https://s3.jcloud.sjtu.edu.cn/1b088ff214b04e6291c549a95685610b-share/deploy-node.sh)
```

## Resources

Add links to external resources for this project, such as CI server, bug tracker, etc.
