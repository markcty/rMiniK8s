#!/bin/bash
if [ -z "$RMINIK8S" ]; then
  echo "Please specify rminik8s source dir"
  exit
fi

# master
multipass launch --name k8s --disk 20G --cpus 4 --mem 4G --cloud-init cloud-init-arm64.yaml lts
# 2 nodes: 2v8g, 6v4g
multipass launch --name k8s1 --disk 20G --cpus 2 --mem 8G --cloud-init cloud-init-arm64.yaml lts
multipass launch --name k8s2 --disk 20G --cpus 6 --mem 4G --cloud-init cloud-init-arm64.yaml lts

multipass mount $RMINIK8S k8s:~/rminik8s
multipass mount $RMINIK8S k8s1:~/rminik8s
multipass mount $RMINIK8S k8s2:~/rminik8s
