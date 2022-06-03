job("Run tests & Build") {
    startOn {
        codeReviewOpened{}
    }

    container(image = "minik8s.xyz/rust:latest") {
        resources {
            cpu = 8.cpu
            memory = 12000.mb
        }

        shellScript {
            interpreter = "/bin/bash"
            content = """
                sudo su
                pwd
                rustup version
                cargo version
                echo Installing clippy...
                rustup component add clippy
                echo Running tests...
                cargo clippy -- -Dwarnings
                cargo test
            """
        }
    }
}

job("Build docker images") {
    startOn {
        gitPush {
            branchFilter {
                +"refs/heads/develop"
            }
        }
    }

    container(displayName = "Compile", image = "minik8s.xyz/rust:latest") {
        resources {
            cpu = 8.cpu
            memory = 12000.mb
        }
        shellScript {
            interpreter = "/bin/bash"
            content = """
                    cargo build --release
                    cd ./target/release
                    curl -F "api_server=@api_server" http://minik8s.xyz:8008/api/upload
                    curl -F "endpoints-controller=@endpoints-controller" http://minik8s.xyz:8008/api/upload
                    curl -F "ingress-controller=@ingress-controller" http://minik8s.xyz:8008/api/upload
                    curl -F "podautoscaler=@podautoscaler" http://minik8s.xyz:8008/api/upload
                    curl -F "replicaset-controller=@replicaset-controller" http://minik8s.xyz:8008/api/upload
                    curl -F "rkube-proxy=@rkube-proxy" http://minik8s.xyz:8008/api/upload
                    curl -F "rkubectl=@rkubectl" http://minik8s.xyz:8008/api/upload
                    curl -F "rkubelet=@rkubelet" http://minik8s.xyz:8008/api/upload
                    curl -F "scheduler=@scheduler" http://minik8s.xyz:8008/api/upload
                    curl -F "function-controller=@function-controller" http://minik8s.xyz:8008/api/upload
                    curl -F "serverless-router=@serverless-router" http://minik8s.xyz:8008/api/upload
                    curl -F "gpujob-controller=@gpujob-controller" http://minik8s.xyz:8008/api/upload
                """
        }
    }

    docker {
        build {
            file = "./scripts/x86/docker/api_server/Dockerfile"
        }
        push("minik8s.xyz/x86/api_server") {
            tags("latest")
        }
    }

    docker {
        build {
            file = "./scripts/x86/docker/endpoints-controller/Dockerfile"
        }
        push("minik8s.xyz/endpoints-controller") {
            tags("latest")
        }
    }

    docker {
        build {
            file = "./scripts/x86/docker/ingress-controller/Dockerfile"
        }
        push("minik8s.xyz/ingress-controller") {
            tags("latest")
        }
    }

    docker {
        build {
            file = "./scripts/x86/docker/podautoscaler/Dockerfile"
        }
        push("minik8s.xyz/podautoscaler") {
            tags("latest")
        }
    }

    docker {
        build {
            file = "./scripts/x86/docker/replicaset-controller/Dockerfile"
        }
        push("minik8s.xyz/replicaset-controller") {
            tags("latest")
        }
    }

    docker {
        build {
            file = "./scripts/x86/docker/scheduler/Dockerfile"
        }
        push("minik8s.xyz/scheduler") {
            tags("latest")
        }
    }

    docker {
        build {
            context = "./scripts/x86/docker/function-controller"
            file = "./scripts/x86/docker/function-controller/Dockerfile"
        }
        push("minik8s.xyz/function-controller") {
            tags("latest")
        }
    }

    docker {
        build {
            context = "./scripts/x86/docker/serverless-router"
            file = "./scripts/x86/docker/serverless-router/Dockerfile"
        }
        push("minik8s.xyz/serverless-router") {
            tags("latest")
        }
    }

    docker {
        build {
            context = "./scripts/x86/docker/gpujob-controller"
            file = "./scripts/x86/docker/gpujob-controller/Dockerfile"
        }
        push("minik8s.xyz/gpujob-controller") {
            tags("latest")
        }
    }
}