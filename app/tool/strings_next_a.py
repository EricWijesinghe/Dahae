# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Next wave, package A: payment gateway disconnect guard and the PortOne test-channel alert. Columns: ko (source), en.
S = {
  "err_gateway_has_refundable_payments": ["이 결제사로 받은 결제 중 아직 환불할 수 있는 결제(결제 후 1년 이내)가 있어 연결을 끊을 수 없어요. 연결을 끊으면 그 환불이 실패해요. 새 결제만 멈추려면 토스페이먼츠는 사용을 끄고, PortOne은 채널 키 칸을 지우세요.", "Payments taken through this gateway can still be refunded (up to a year after payment), so it can't be disconnected: those refunds would fail. To stop new payments only, turn Toss Payments' Active off, or clear PortOne's channel key slots."],
  "oRuleTestChannel": ["PortOne 테스트 채널로 들어온 결제", "Payment on a PortOne test channel"],
  "oCheckTestChannel": ["PortOne 테스트 채널 결제", "PortOne test-channel payments"],
  "oActTestChannel": ["PortOne 테스트 채널로 결제나 카드 저장이 들어왔어요. 실제 돈은 오가지 않았어요. 이 서버의 키가 아닌 테스트 채널로 들어온 건은 거절해서 예약되지 않았어요. 테스트 키로 시험 중이면 괜찮아요. 운영 중이라면 설정 > 결제 (PortOne)의 채널 키가 라이브 채널 키인지 확인하고, PortOne 관리자 화면에서 이 상점의 테스트 채널을 삭제한 뒤, 테스트 결제 내역에서 그 결제를 찾아 예약을 고객과 다시 확인하세요.", "A payment or card save came in through a PortOne test channel, so no real money moved. Ones on a test channel whose key this server does not hold were refused and booked nothing. If you are testing with test keys, this is fine. If you are live, make sure the channel keys under Setup > Payments (PortOne) are live channel keys, delete this store's test channels in the PortOne console, then find the payment in PortOne's test payment history and check the booking with the customer."],
}
