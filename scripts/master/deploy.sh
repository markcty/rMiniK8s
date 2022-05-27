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
  wget -q --show-progress -O /usr/local/bin/flanneld "https://github.com/flannel-io/flannel/releases/download/v0.17.0/flanneld-${ARCH}"
  chmod +x /usr/local/bin/flanneld
else
  printf "flanneld installed\n\n"
fi
if [ ! -f /etc/systemd/system/flanneld.service ]; then
  wget -q --show-progress -O /etc/systemd/system/flanneld.service https://s3.jcloud.sjtu.edu.cn/1b088ff214b04e6291c549a95685610b-share/flanneld.service
fi
printf "flanneld service installed\n\n"

# start etcd
printf "Starting etcd... \n"
docker run -d \
  --network host \
  --restart=always \
  --name etcd quay.io/coreos/etcd:latest \
  etcd \
  -enable-v2 \
  -advertise-client-urls http://0.0.0.0:2379 \
  -listen-client-urls http://0.0.0.0:2379
docker run -d \
  --network host \
  --rm \
  quay.io/coreos/etcd:latest \
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
  sleep 0.1
done
set -a; source /run/flannel/subnet.env; set +a
printf "flanneld service ok\n\n"

# assign ip for each component
SUBNET_BASE=${FLANNEL_SUBNET:0:-5}
export API_SERVER_IP=${SUBNET_BASE}.100
export INGRESS_IP=${SUBNET_BASE}.101
export FUNCTION_ROUTER_IP=${SUBNET_BASE}.102
export PROMETHEUS_IP=${SUBNET_BASE}.103

# start dns
envsubst <./dns/function_router.db.template > ./dns/function_router.db
envsubst <./dns/ingress.db.template > ./dns/ingress.db
docker run -d --name dns \
  --restart=always \
  -v $(pwd)/dns:/config \
  -p 53:53/udp \
  coredns/coredns \
  -conf /config/Corefile
printf "nameserver $IP\n" > /etc/resolv.conf

# start control plane
docker-compose -p minik8s-control-plane up -d 
printf "control plane started\n\n"

# start rkube-proxy
printf "Installing rkube-proxy ...\n"
wget -q --show-progress -O /usr/local/bin/rkube-proxy http://minik8s.xyz:8008/rkube-proxy
chmod +x /usr/local/bin/rkube-proxy
if [ ! -f /etc/systemd/system/rkubeproxy.service ]; then
  wget -q --show-progress -O /etc/systemd/system/rkubeproxy.service http://minik8s.xyz:8008/rkubeproxy.service
fi
printf "ETCD_ENDPOINT=$ETCD_ENDPOINT\nAPI_SERVER_ENDPOINT=http://${API_SERVER_IP}:8080\n" > /etc/rminik8s/node.env
systemctl daemon-reload
systemctl restart rkubeproxy.service
printf "rkube-proxy started\n\n"

# display ip
printf "The following endpoints are exposed:\n"
printf "    api-server: http://${API_SERVER_IP}:8080\n"
printf "    ingress:    http://${INGRESS_IP}\n"
printf "    prometheus: http://${PROMETHEUS_IP}:9090\n"
printf "    etcd:       $ETCD_ENDPOINT\n"
printf "    dns:        $IP:53\n"

