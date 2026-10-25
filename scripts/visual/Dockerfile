FROM node:26-bookworm-slim AS node26

FROM mcr.microsoft.com/playwright:v1.64.1-noble@sha256:47b510dd9230bf0b9bfc9b20102241e577ea7a99334ab4c3ca19f19770e9b8ea
COPY --from=node26 /usr/local/bin/node /usr/local/bin/node
COPY --from=node26 /usr/local/lib/node_modules/npm /usr/local/lib/node_modules/npm
RUN ln -sfn ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && ln -sfn ../lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx
WORKDIR /work/viewer-ui
ENTRYPOINT ["node", "/work/scripts/visual/run.ts"]
