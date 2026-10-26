# Caller admission 제안 (적용하지 않음)

상태: 제안만. 이 문서는 어떤 검증도 바꾸지 않는다.

## 문제

academy-workbench `.github/source-author.json`이 2026-10-21의 native-codex thread 하나를 모든 PR의 caller admission으로 고정하고 있다. 그 결과 다른 runtime이 올린 PR은 내용이 맞아도 caller 단계에서 즉시 거부된다. 작성자 신원이 신뢰의 닻이 되면 새 CLI가 합류할 때마다 고정값을 바꿔야 하고, 그 자체가 브랜드 의존이다.

## 제안 형태

- caller 검증은 repo, projectId, workflow, head SHA 결박으로 한다. 호출이 어느 repo의 어느 workflow에서 어느 head를 대상으로 하는지만 검증한다.
- author descriptor(runtime, taskId)는 receipt와 로그에 기록만 한다. 고정값과 비교하지 않는다.
- 신뢰의 닻은 고정 publisher의 인증된 status와 정확한 digest 결박으로 유지한다. caller를 넓히는 것이 아니라, 판정 대상을 브랜드에서 내용 결박으로 옮기는 것이다.
- 기존 native-codex로 들어온 기록은 그대로 유효하다. descriptor 기록은 이미 그 형태를 담을 수 있다.

## 적용 조건

- 이 제안은 소유자 결정 후 별도 PR로만 적용한다. 그 전까지 source-author 고정은 현행 유지다.
- 적용 시 fixture: 처음 보는 runtime descriptor의 caller 통과, head SHA 불일치 거부, repo/projectId 불일치 거부를 보인다.
