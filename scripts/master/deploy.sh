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
elif 
  echo "flanneld installed"
fi

# start etcd
docker run -d --rm -p 2379:2379 \
  --name etcd quay.io/coreos/etcd:latest \
  etcd \
  -enable-v2 \
  -advertise-client-urls http://0.0.0.0:2379 \
  -listen-client-urls http://0.0.0.0:2379

source /run/flannel/subnet.env
ETCD_ENDPOINT=http://${IP}:2379 FLANNEL_SUBNET=10.5.89.1/24 docker-compose up -d

# start flanneld
systemctl restart flanneld.service

