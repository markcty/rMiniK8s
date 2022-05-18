#!/bin/bash
if [ "$EUID" -ne 0 ]
  then echo "Please run as root"
  exit
fi

# stop docker
systemctl stop docker.service containerd.service docker.socket

# env
ARCH=$(dpkg --print-architecture)
echo "ARCH: $ARCH"

# install flannel
if [ ! -f /usr/local/bin/flanneld ]; then
  echo "Installing flanneld ..."
  wget -q --show-progress -O /usr/local/bin/flanneld "https://github.com/flannel-io/flannel/releases/download/v0.17.0/flanneld-${ARCH}"
  chmod +x /usr/local/bin/flanneld
fi

# install services
echo "Installing systemd services ..."
wget -q --show-progress -O /etc/systemd/system/flanneld.service https://s3.jcloud.sjtu.edu.cn/1b088ff214b04e6291c549a95685610b-share/flanneld.service
wget -q --show-progress -O /etc/systemd/system/kdocker.service https://s3.jcloud.sjtu.edu.cn/1b088ff214b04e6291c549a95685610b-share/kdocker.service

# write configuration file
if [ ! -f /etc/rminik8s/conf.env ]; then
  echo "Configuration not found. Generating..."
  echo -n "Please input master etcd endpoint(eg. http://127.0.0.1:2379): "
  read -r ETCD_ENDPOINT
  mkdir -p /etc/rminik8s
  printf 'ETCD_ENDPOINT="%s"\n' "$ETCD_ENDPOINT" > /etc/rminik8s/conf.env
fi

# modify dns
systemctl stop systemd-resolved
if grep -Fq "minik8s" /etc/resolv.conf;
then
  echo "DNS already configured"
else
  echo -n "Please input DNS server ip: "
  read -r DNS_SERVER_IP
  mv /etc/resolv.conf /etc/resolv.conf.bk
  printf '#minik8s\nnameserver %s\n' "$DNS_SERVER_IP" | cat - /etc/resolv.conf.bk > /etc/resolv.conf
fi


# start
systemctl daemon-reload
systemctl restart flanneld.service
echo "flanneld started"

while [ ! -f /run/flannel/subnet.env ]; do
  sleep 1
done
systemctl restart kdocker.service
echo "docker started"
