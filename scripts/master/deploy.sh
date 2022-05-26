#!/bin/bash
if [ "$EUID" -ne 0 ]
  then echo "Please run as root"
  exit
fi

ARCH=$(dpkg --print-architecture)
echo "ARCH: $ARCH"
IP=$(ip route get 114.114.114.114 | awk '{ print $7; exit }')
echo "IP: $IP"

# install flannel
if [ ! -f /usr/local/bin/flanneld ]; then
  echo "Installing flanneld ..."
  wget -q --show-progress -O /usr/local/bin/flanneld "https://github.com/flannel-io/flannel/releases/download/v0.17.0/flanneld-${ARCH}"
  chmod +x /usr/local/bin/flanneld
else
  echo "flanneld installed"
fi
if [ ! -f /usr/local/bin/flanneld ]; then
  wget -q --show-progress -O /etc/systemd/system/flanneld.service https://s3.jcloud.sjtu.edu.cn/1b088ff214b04e6291c549a95685610b-share/flanneld.service
fi

# start etcd
docker run -d --rm -p 2379:2379 \
  --name etcd quay.io/coreos/etcd:latest \
  etcd \
  -enable-v2 \
  -advertise-client-urls http://0.0.0.0:2379 \
  -listen-client-urls http://0.0.0.0:2379
export ETCD_ENDPOINT=http://${IP}:2379
printf 'ETCD_ENDPOINT="%s"\n' "$ETCD_ENDPOINT" > /etc/rminik8s/conf.env

# start flannel
systemctl daemon-reload
systemctl restart flanneld.service
while [ ! -f /run/flannel/subnet.env ]; do
  sleep 0.1
done
echo "flanneld started"

# start control plane
set -a; source /run/flannel/subnet.env; set +a
docker-compose up -d -p minik8s-control-plane
echo "control plane started"

# display ip
echo "The following endpoints are exposed"
echo "  api-server: http://${IP}:8080"
echo "  prometheus: http://${IP}:9090"
echo "  ingress:    http://${IP}:8081"

# start rkube-proxy
if [ ! -f /usr/local/bin/rkube-proxy ]; then
  wget -q --show-progress -O /usr/local/bin/rkube-proxy http://minik8s.xyz:8008/rkube-proxy
  chmod +x /usr/local/bin/rkube-proxy
fi
if [ ! -f /etc/systemd/system/rkubeproxy.service ]; then
  wget -q --show-progress -O /etc/systemd/system/rkubeproxy.service http://minik8s.xyz:8008/rkubeproxy.service
fi
if [ ! -f /etc/rminik8s/node.env ]; then
  printf 'ETCD_ENDPOINT="%s"\nAPI_SERVER_ENDPOINT=http://127.0.0.1:8080\n' "$ETCD_ENDPOINT" > /etc/rminik8s/node.env
fi
systemctl daemon-reload
systemctl restart rkubeproxy.service
