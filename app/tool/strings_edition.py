# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Console: the update edition card (Settings > Server). Shown only on a server the installer offered another edition.
# Columns: ko (source), en.
# i18n-archive: extra languages archived in archive/i18n-extra-languages (see README to restore)
# {name} placeholders must match across languages (checked by gen_arb.py).
S = {
    "ednTitle": ["업데이트 판 (Edition)", "Update edition"],
    "ednBody": ["지금 판: {current}. 판을 바꾸면 이 서버가 다른 릴리스의 업데이트를 받아요. 사장님 코드와 서버 승인 코드가 필요해요.", "Current edition: {current}. Switching makes this server take its updates from another release. Needs your owner code and the server approval code."],
    "ednSwitch": ["{name}(으)로 바꾸기", "Switch to {name}"],
    "ednConfirm": ["먼저 백업해요. 그다음부터 서버가 {name}의 릴리스를 따라가요. 데이터, 키, 도메인, 스토어 앱은 그대로예요. '이전 버전으로 되돌리기'는 계속 쓸 수 있어요. 몇 분 동안 서비스가 다시 시작돼요.", "It backs up first. From then on the server follows {name}'s releases. Data, keys, domain and store apps stay as they are. 'Roll back to the previous version' stays available. The service restarts for a few minutes."],
    "ednRunning": ["판을 바꾸고 있어요: 백업, 확인, 바꾸기, 응답 확인 순서예요. 이 화면을 닫아도 서버에서 계속돼요.", "Switching: backup, check, switch, health check. It keeps going on the server even if you close this screen."],
    "ednDone": ["{name}(으)로 바꿨어요.", "Switched to {name}."],
    "ednRolledBack": ["새 판이 응답하지 않아 되돌렸어요. 그대로 {current}이에요.", "The new edition did not answer, so it went back. Still on {current}."],
    "ednFailed": ["판을 바꾸지 못했어요: {reason}", "Could not switch the edition: {reason}"],
    "ednNoSummary": ["서버가 보낸 요약이 판 바꾸기가 아니어서 승인 코드를 보내지 않았어요. 아래 최근 작업을 확인해 주세요.", "The server's summary was not an edition switch, so no approval code was sent. Check the recent jobs below."],
    "ednCancelled": ["서버 승인 코드를 넣지 않아서 판을 바꾸지 않았어요. 기다리던 작업은 10분 뒤에 저절로 취소돼요.", "No server approval code was entered, so the edition was not switched. The held job cancels itself after 10 minutes."],
    "ednToolsStale": ["관리 도구는 아직 예전 것이에요. 위의 '지금 업데이트'를 한 번 눌러 주세요.", "The server tools are still the old ones. Press 'Update now' above once."],
    "ednKind": ["업데이트 판 바꾸기", "Edition switch"],
}
