# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Next wave, package A: payment gateway disconnect guard and the PortOne test-channel alert. Columns: ko (source), en.
S = {
  "err_gateway_has_refundable_payments": ["이 결제사로 받은 결제 중 아직 환불할 수 있는 결제(결제 후 1년 이내)가 있어 연결을 끊을 수 없어요. 연결을 끊으면 그 환불이 실패해요. 새 결제만 멈추려면 토스페이먼츠는 사용을 끄고, PortOne은 채널 키 칸을 지우세요.", "Payments taken through this gateway can still be refunded (up to a year after payment), so it can't be disconnected: those refunds would fail. To stop new payments only, turn Toss Payments' Active off, or clear PortOne's channel key slots."],
  "oRuleTestChannel": ["PortOne 테스트 채널로 들어온 결제", "Payment on a PortOne test channel"],
  "oCheckTestChannel": ["PortOne 테스트 채널 결제", "PortOne test-channel payments"],
  "oActTestChannel": ["실제 돈이 오가지 않은 결제나 저장된 카드가 있어요. 라이브 서버에 PortOne 테스트 채널 키가 들어가 있어요. 설정 > 결제 (PortOne)에서 채널 키를 라이브 채널 키로 바꾸고, 해당 예약의 결제를 고객과 다시 확인하세요.", "A payment or saved card went through a PortOne test channel, so no real money moved: the live server has a PortOne test channel key. Replace it with the live channel key in Settings > Payments (PortOne), and check those bookings' payments with the customers."],
}
