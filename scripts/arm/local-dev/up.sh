#!/bin/bash
if [ "$EUID" -ne 0 ]
  then echo "Please run as root"
  exit
fi

mkdir -p /etc/rminik8s
ARCH=$(dpkg --print-architecture)
printf "ARCH: $ARCH\n"
export IP=$(ip route get 114.114.114.114 | awk '{ print $7; exit }')
printf "IP: $IP\n\n"

# install flannel
if [ ! -f /usr/local/bin/flanneld ]; then
  printf "Installing flanneld ...\n"
  wget -q --show-progress -O /usr/local/bin/flanneld "http://minik8s.xyz:8008/flanneld-$ARCH"
  chmod +x /usr/local/bin/flanneld
else
  printf "flanneld installed\n\n"
fi
cp ./flanneld.service /etc/systemd/system/flanneld.service
printf "flanneld service installed\n\n"

# start etcd
printf "Starting etcd... \n"
docker run -d \
  --network host \
  --restart=always \
  --name etcd quay.io/coreos/etcd:v3.5.4-arm64 \
  etcd \
  -enable-v2 \
  -advertise-client-urls http://0.0.0.0:2379 \
  -listen-client-urls http://0.0.0.0:2379
docker run -d \
  --network host \
  --rm \
  -e ETCDCTL_API=2 \
  quay.io/coreos/etcd:v3.5.4-arm64 \
  etcdctl \
  set /coreos.com/network/config '{ "Network": "10.66.0.0/16", "Backend": {"Type": "vxlan"}}'
export ETCD_ENDPOINT=http://${IP}:2379
printf "ETCD_ENDPOINT=$ETCD_ENDPOINT\n" > /etc/rminik8s/conf.env
printf "ETCD started, endpoint=$ETCD_ENDPOINT\n\n"

# start flannel
systemctl daemon-reload
systemctl restart flanneld.service
while [ ! -f /run/flannel/subnet.env ]; do
  echo "Waiting for flanneld service"
  sleep 0.5
done
set -a; source /run/flannel/subnet.env; set +a
printf "flanneld service ok\n\n"

# assign ip for each component
SUBNET_BASE=${FLANNEL_SUBNET:0:-5}
export API_SERVER_IP=${SUBNET_BASE}.100
export INGRESS_IP=${SUBNET_BASE}.101
export SERVERLESS_ROUTER_IP=$IP
export PROMETHEUS_IP=${SUBNET_BASE}.103

# start dns
systemctl stop systemd-resolved
printf "nameserver 119.29.29.29\n" > /etc/resolv.conf
envsubst <./dns/serverless_router.db.template > ./dns/serverless_router.db
envsubst <./dns/ingress.db.template > ./dns/ingress.db
if [ ! -f /usr/local/bin/coredns ]; then
  printf "Installing coredns ...\n"
  wget -q --show-progress -O /usr/local/bin/coredns http://minik8s.xyz:8008/coredns
  chmod +x /usr/local/bin/coredns
else
  printf "coredns installed\n\n"
fi
mkdir -p /config
cp ./dns/* /config
cp ./coredns.service /etc/systemd/system/coredns.service
systemctl daemon-reload
systemctl restart coredns.service
printf "nameserver $IP\n" > /etc/resolv.conf
printf "DNS server started\n\n"

# configure docker
envsubst < ./daemon.example.json > ./daemon.json
mv ./daemon.json /etc/docker/daemon.json
systemctl restart docker

docker run --rm -d --net=host \
  -v $(pwd)/prometheus.yml:/etc/prometheus/prometheus.yml \
  --name prometheus \
  prom/prometheus:latest \
  --web.enable-lifecycle --config.file=/etc/prometheus/prometheus.yml

docker run \
  --volume=/:/rootfs:ro \
  --volume=/var/run:/var/run:rw \
  --volume=/sys:/sys:ro \
  --volume=/var/lib/docker/:/var/lib/docker:ro \
  --publish=8090:8080 \
  --detach=true \
  --name=cadvisor \
  zcube/cadvisor:latest