# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Toss Payments as a payment gateway, alone or next to PortOne (owner console setup card). Columns: ko (source), en.
S = {
  "cIntTossPayments": ["결제 (토스페이먼츠)", "Payments (Toss Payments)"],
  "cSetupGatewayLive": ["결제를 받는 곳: {name}", "Payments go to: {name}"],
  "cSetupGatewayNone": ["지금 결제를 받는 곳이 없어요. PortOne이나 토스페이먼츠를 설정하거나, 토스페이먼츠의 사용을 켜세요.", "No gateway takes payments right now. Set up PortOne or Toss Payments, or turn Toss Payments' Active on."],
  "cPayMethodGateway": ["결제사: {name}", "Gateway: {name}"],
  "cSetupGatewayTestKeys": ["(테스트 키: 실제 돈이 오가지 않아요)", "(test keys: no real money moves)"],
  "cSetupBillingInvalidKey": ["자동결제(빌링) 키가 올바르지 않아요. 빌링 MID의 키를 확인해 주세요", "The billing keys were rejected. Check the billing MID's keys"],
  "err_invalid_client_key": ["클라이언트 키는 test_ck_ 또는 live_ck_로 시작해야 해요.", "The client key starts with test_ck_ or live_ck_."],
  "err_gateway_in_use": ["이 결제사에 진행 중인 결제, 저장된 카드나 남은 환불이 있어 연결을 끊을 수 없어요. 새 결제만 멈추려면 토스페이먼츠는 사용을 끄고, PortOne은 채널 키 칸을 지우세요.", "This gateway still has payments in progress, saved cards or refunds, so it can't be disconnected. To stop new payments only, turn Toss Payments' Active off, or clear PortOne's channel key slots."],
  "err_key_mode_mismatch": ["테스트 키와 라이브 키를 섞을 수 없어요. 같은 종류의 키를 넣어 주세요.", "Test and live keys can't be mixed. Use keys of one kind."],
}
