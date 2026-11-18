# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Toss Payments as a second payment gateway (owner console setup card). Columns: ko (source), en.
S = {
  "cIntTossPayments": ["결제 (토스페이먼츠)", "Payments (Toss Payments)"],
  "cSetupGatewayLive": ["새 결제를 받는 곳: {name}", "New payments go to: {name}"],
  "cSetupGatewayTestKeys": ["(테스트 키: 실제 돈이 오가지 않아요)", "(test keys: no real money moves)"],
  "cSetupBillingInvalidKey": ["자동결제(빌링) 키가 올바르지 않아요. 빌링 MID의 키를 확인해 주세요", "The billing keys were rejected. Check the billing MID's keys"],
  "err_invalid_client_key": ["클라이언트 키는 test_ck_ 또는 live_ck_로 시작해야 해요.", "The client key starts with test_ck_ or live_ck_."],
  "err_key_mode_mismatch": ["테스트 키와 라이브 키를 섞을 수 없어요. 같은 종류의 키를 넣어 주세요.", "Test and live keys can't be mixed. Use keys of one kind."],
}
