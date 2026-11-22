# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Console: AWS automatic offsite backup card label, server recovery card (rollback, key copy download).
# Columns: ko (source), en.
# i18n-archive: extra languages archived in archive/i18n-extra-languages (see README to restore)
# {name} placeholders must match across languages (checked by gen_arb.py).
S = {
    "cSetupAwsAuto": ["AWS S3 (자동)", "AWS S3 (automatic)"],
    "cSetupNoInstanceRole": ["서버 역할로 AWS 저장소에 접속하지 못했어요. AWS 템플릿으로 만든 서버인지, AWS 관리자가 launch.sh --admin 을 한 번 다시 실행했는지 확인하세요.", "The server role could not reach the AWS bucket. Check that this server was made with the AWS template and that the AWS admin ran launch.sh --admin once more."],
    "srTitle": ["복구", "Recovery"],
    "srRollbackBody": ["업데이트 뒤에 문제가 생기면 바로 전 버전으로 돌아가요. 먼저 백업하고, 데이터는 그대로예요. 토스 결과를 기다리는 정산이 있으면 되돌리지 않아요. 되돌린 뒤에는 새 버전이 나올 때까지 자동 업데이트가 그 버전을 다시 올리지 않아요. 사장님 코드와 서버 승인 코드가 필요해요.", "If something breaks after an update, go back to the version before. It backs up first and keeps your data. It does not run while a Toss payout is still waiting for its result. After a rollback, automatic updates leave that version alone until a newer one comes out. Needs your owner code and the server approval code."],
    "srRollbackButton": ["이전 버전으로 되돌리기", "Roll back to the previous version"],
    "srRollbackConfirm": ["이전 버전으로 되돌릴까요? 몇 분 동안 서비스가 다시 시작돼요.", "Roll back to the previous version? The service restarts for a few minutes."],
    "srRunning": ["진행 중이에요. 이 화면을 닫아도 서버에서 계속돼요.", "Running. It keeps going on the server even if you close this screen."],
    "srRollbackDone": ["이전 버전으로 되돌렸어요. 새 버전이 나올 때까지 자동 업데이트는 이 버전을 지켜요.", "Rolled back to the previous version. Automatic updates keep it until a newer version comes out."],
    "srApprovalCancelled": ["서버 승인 코드를 넣지 않아서 되돌리지 않았어요. 기다리던 작업은 10분 뒤에 저절로 취소돼요.", "No server approval code was entered, so nothing was rolled back. The held job cancels itself after 10 minutes."],
    "srApprovalNoSummary": ["서버가 보낸 요약이 되돌리기가 아니어서 승인 코드를 보내지 않았어요. 아래 최근 작업을 확인해 주세요.", "The server's summary was not a rollback, so no approval code was sent. Check the recent jobs below."],
    "srStillRunning": ["아직 진행 중이에요. 결과는 아래 최근 작업에서 보세요.", "Still running. See the result under recent jobs below."],
    "srOldTools": ["서버의 관리 도구가 예전 버전이라 이 작업을 몰라요. 설치 줄을 한 번 다시 실행하세요 (안내서 10 C-5).", "The server's tools are older and do not know this job. Run the install line once more (guide 10 C-5)."],
    "srFailed": ["되돌리지 못했어요: {reason}", "Could not roll back: {reason}"],
    "srKeysTitle": ["비밀 키 사본 내려받기", "Download the secret key copy"],
    "srKeysBody": ["서버를 잃어도 외부 백업을 풀 수 있게, 서버의 비밀 키를 비밀번호로 잠근 파일을 내려받아요. 파일과 비밀번호는 서버 밖에 따로 보관하세요. 만든 파일은 한 번만 내려받을 수 있어요.", "Download the server's secret keys locked with a passphrase, so offsite backups can be opened even if the server is lost. Keep the file and the passphrase apart, off the server. Each copy downloads once."],
    "srKeysPass": ["잠금 비밀번호 (12자 이상)", "Passphrase (12 characters or more)"],
    "srKeysPassAgain": ["잠금 비밀번호 한 번 더", "Passphrase again"],
    "srKeysPassShort": ["12자 이상 입력하세요", "Use 12 characters or more"],
    "srKeysPassMismatch": ["두 비밀번호가 달라요", "The two passphrases differ"],
    "srKeysButton": ["만들고 내려받기", "Make and download"],
    "srKeysRunning": ["비밀 키 사본을 만들고 있어요", "Making the secret key copy"],
    "srKeysDone": ["서버 밖에 보관했다고 기록했어요.", "Recorded as kept off the server."],
    "srKeysKept": ["서버 밖에 보관했어요", "Kept off the server"],
    "srKeysKeptConfirm": ["내려받은 파일을 USB 나 비밀번호 관리자처럼 서버 밖에 옮겨 두었고, 비밀번호도 따로 적어 두었나요? 그래야 서버를 잃어도 백업을 풀 수 있어요.", "Did you move the downloaded file off the server (a USB stick or a password manager) and write the passphrase down separately? Only then can backups be opened if the server is lost."],
    "srKeysNotKept": ["내려받았어요. 서버 밖 보관은 아직 기록하지 않았어요: 옮겨 둔 뒤 사본을 다시 만들어 확인해 주세요.", "Downloaded. Not yet recorded as kept off the server: move it, then make a new copy to confirm."],
    "srKeysSaveFailed": ["파일을 저장하지 못했어요. 사본을 다시 만들어 주세요.", "The file could not be saved. Make a new copy."],
    "srKeysFailed": ["사본을 만들지 못했어요: {reason}", "Could not make the copy: {reason}"],
    "srKindRollback": ["이전 버전으로 되돌리기", "Roll back"],
    "srKindKeysBackup": ["비밀 키 사본", "Secret key copy"],
    "srKindKeysConfirm": ["비밀 키 사본 보관 기록", "Key copy recorded"],
}
