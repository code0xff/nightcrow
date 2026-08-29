# viewer-ui scope

저장소 공통 작업 흐름은 [루트 AGENTS.md](../AGENTS.md)를 따른다. 이 문서는 viewer와 서버 사이의 계약, 번들, 개발 서버처럼 이 디렉터리에서만 놓치기 쉬운 경계만 다룬다.

## 프론트엔드 계약

- `viewer-ui/src/api.ts`와 `viewer-ui/src/api/`의 HTTP, SSE, WebSocket 타입·인코더·디코더는 `src/web/viewer/dto/`와 서버 terminal protocol의 반대편이다. 필드, enum variant, 메시지 순서 또는 경로를 바꾸면 양쪽 구현과 해당 contract/integration test를 함께 갱신한다.
- `api.fixture.json`은 Rust DTO에서 생성되는 커밋 대상 wire fixture다. Rust payload가 바뀌면 저장소 루트에서 `UPDATE_API_FIXTURE=1 cargo test the_wire_fixture`로 재생성하고, fixture diff를 검토한 뒤 TypeScript API 타입을 맞춘다. fixture를 임의로 손으로 고쳐 계약 drift를 숨기지 않는다.
- API 계약 변경은 [공통 Verify 절차](../docs/getting-started.md#building-and-testing)의 frontend 게이트로 확인한다. `api.contract.test.ts`의 타입 대입은 누락·이름 변경·타입 변경을 잡고, Rust fixture test는 서버가 추가하거나 제거한 payload를 고정한다.

## 번들 및 개발 서버

- `viewer-ui/dist/`는 Vite가 생성하는 커밋 대상 릴리스 번들이며 Rust 서버가 바이너리에 임베드한다. 번들에 영향을 주는 소스·설정·public asset 변경 뒤에는 반드시 build하고, 최종 변경에는 소스와 일치하는 `dist` 결과를 포함한다. clean checkout에서 같은 build를 다시 실행해 `dist`에 미커밋 차이가 없어야 한다.
- `vite.config.ts`의 relative asset base와 `/api`, `/login`, `/ws` 개발 프록시는 임베드 서버와 Vite 개발 서버 사이의 배포 계약이다. mount path나 서버 포트를 바꿀 때는 Rust route와 문서·검증을 함께 확인한다.
- 화면 조립은 `pages/`, 재사용 UI는 `components/`, 상태·효과는 `hooks/`, API 외 순수 로직은 `lib/`에 둔다. 서버 wire 문자열을 각 hook에서 다시 해석하지 말고 `api/`의 경계에서 검증된 타입을 전달한다.
