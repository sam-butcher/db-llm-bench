# TypeDB console for seeding (the server image ships no console). Keep the
# console version aligned with the typedb service's image tag.
FROM debian:bookworm-slim

ARG TYPEDB_CONSOLE_VERSION=3.11.5
ARG TARGETARCH

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN case "$TARGETARCH" in \
        amd64) ARCH=x86_64 ;; \
        arm64) ARCH=arm64 ;; \
        *) echo "unsupported architecture: $TARGETARCH" && exit 1 ;; \
    esac \
    && NAME="typedb-console-linux-${ARCH}" \
    && curl -sSL "https://repo.typedb.com/public/public-release/raw/names/${NAME}/versions/${TYPEDB_CONSOLE_VERSION}/${NAME}-${TYPEDB_CONSOLE_VERSION}.tar.gz" \
        | tar xz -C /opt \
    && ln -s "/opt/${NAME}-${TYPEDB_CONSOLE_VERSION}/typedb" /usr/local/bin/typedb

COPY reset.tqls load.tqls seed.sh /
RUN chmod +x /seed.sh

ENTRYPOINT ["/seed.sh"]
