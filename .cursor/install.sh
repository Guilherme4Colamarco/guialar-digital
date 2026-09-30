#!/usr/bin/env bash
set -euo pipefail

# GuiaLar Digital environment bootstrap.
# Installs the toolchain (Java 17 + Apache Ant) used by the project's Ant build
# and produces the executable JAR. Safe to run repeatedly.

JAVA_17_HOME="/usr/lib/jvm/java-17-openjdk-amd64"

if ! command -v ant >/dev/null 2>&1 \
    || ! dpkg -s openjdk-17-jdk-headless >/dev/null 2>&1 \
    || ! command -v dig >/dev/null 2>&1; then
    sudo apt-get update -qq
    sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ant \
        openjdk-17-jdk-headless \
        dnsutils
fi

# Build against Java 17 to match the project's source/target level and CI.
if [ -d "${JAVA_17_HOME}" ]; then
    export JAVA_HOME="${JAVA_17_HOME}"
fi

ant clean jar

echo "GuiaLar Digital build complete: build/jar/guialar-digital.jar"
