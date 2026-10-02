# Guardrails

- 소스·테스트 파일(Rust, TypeScript, TSX, JavaScript)은 300줄 이하로 유지한다. 생성물과 벤더링한 서드파티는 제외한다.
- macOS, Linux, Windows를 모두 지원한다. 플랫폼 차이는 [architecture.md의 platform seam](../../docs/architecture.md#cross-cutting-invariants) 뒤에 두고, 대응물이 없는 제한은 문서에 남긴다.
- 플랫폼 한정 테스트에는 제외 사유를 남긴다. Windows 작업의 Unix 검증도 [검증 절차](../../docs/getting-started.md#building-and-testing)를 따른다.
- 코드 주석은 영어로 쓰고 비자명한 이유·제약만 설명한다.
- 오류는 처리하거나 전파한다. 외부 호출 실패와 잘린 입력·결과를 성공처럼 취급하지 않는다.
