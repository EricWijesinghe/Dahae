// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

// Sample data for the manual screenshots. Names, phone numbers and keys are made up;
// secrets show only as the server's mask (••••).

import 'package:core/core.dart';
import 'package:test_support/test_support.dart';

import '../support/console_host.dart';

Json field(
  String name,
  String kind, {
  bool required = false,
  bool set = false,
  Object? preview,
}) => {
  'name': name,
  'kind': kind,
  'required': required,
  'set': set,
  'preview': preview,
};

/// One setup card the way GET /admin/setup sends it.
Json setupCard(
  String code,
  List<Json> fields, {
  bool configured = false,
  bool locked = false,
  String? unlockUntil,
  Json? lastTest,
  String? mode,
}) => {
  'code': code,
  'can_test': true,
  'source': configured ? 'console' : 'none',
  'configured': configured,
  'locked': locked,
  'unlock_expires_at': unlockUntil,
  'mode': mode,
  'fields': fields,
  'last_test': lastTest,
};

/// The fields each card has (backend integrations.rs), nothing saved.
final cardFields = <String, List<Json>>{
  'portone': [
    field('store_id', 'text', required: true),
    field('api_secret', 'secret', required: true),
    field('webhook_secret', 'secret'),
    field('channel_keys', 'map'),
    field('cash_receipts', 'toggle'),
  ],
  'tosspayments': [
    field('client_key', 'text', required: true),
    field('secret_key', 'secret', required: true),
    field('billing_client_key', 'text'),
    field('billing_secret_key', 'secret'),
    field('security_key', 'secret'),
    field('active', 'toggle'),
  ],
  'solapi': [
    field('api_key', 'secret', required: true),
    field('api_secret', 'secret', required: true),
    field('sender', 'text', required: true),
    field('pf_id', 'text'),
    field('templates', 'map'),
  ],
  'push': [field('service_account', 'file', required: true)],
  'kakao': [
    field('app_id', 'text', required: true),
    field('admin_key', 'secret'),
  ],
  'naver': [
    field('client_id', 'text', required: true),
    field('client_secret', 'secret', required: true),
  ],
  'google': [field('client_ids', 'list', required: true)],
  'apple': [
    field('client_ids', 'list', required: true),
    field('team_id', 'text'),
    field('key_id', 'text'),
    field('private_key', 'file'),
  ],
  'ai': [
    field('api_key', 'secret', required: true),
    field('model', 'text'),
    field('mode', 'text'),
    field('thread_hourly_cap', 'text'),
    field('member_daily_cap', 'text'),
    field('daily_cap', 'text'),
  ],
  'backup': [
    field('endpoint', 'text', required: true),
    field('region', 'text', required: true),
    field('bucket', 'text', required: true),
    field('access_key', 'secret', required: true),
    field('secret_key', 'secret', required: true),
    field('retention_days', 'text'),
  ],
  'payout': [
    field('mode', 'text', required: true),
    field('secret_key', 'secret'),
    field('security_key', 'secret'),
  ],
  'juso': [field('confm_key', 'secret', required: true)],
};

/// A saved PortOne card (store ID and secret saved, card channel key saved).
List<Json> savedPortoneFields() => [
  field(
    'store_id',
    'text',
    required: true,
    set: true,
    preview: 'store-1a2b…9f0e',
  ),
  field('api_secret', 'secret', required: true, set: true, preview: '••••'),
  field('webhook_secret', 'secret', set: true, preview: '••••'),
  field('channel_keys', 'map', set: true, preview: {'card': '••••'}),
  field('cash_receipts', 'toggle'),
];

List<Json> savedTossFields({bool active = false}) => [
  field(
    'client_key',
    'text',
    required: true,
    set: true,
    preview: 'test_ck_••••',
  ),
  field('secret_key', 'secret', required: true, set: true, preview: '••••'),
  field('billing_client_key', 'text'),
  field('billing_secret_key', 'secret'),
  field('security_key', 'secret', set: true, preview: '••••'),
  field('active', 'toggle', set: active, preview: active),
];

const webhookUrl = 'https://api.example.co.kr/v1/webhooks/portone';
const tossPayoutWebhookUrl = 'https://api.example.co.kr/v1/webhooks/toss';
const tossPaymentsWebhookUrl =
    'https://api.example.co.kr/v1/webhooks/tosspayments';

/// GET /admin/setup on a fresh install: nothing saved yet.
Json freshSetup() => {
  'business': const <String, Object?>{},
  'review_account': null,
  'heartbeat_configured': false,
  'integrations': [
    for (final code in const [
      'portone',
      'tosspayments',
      'solapi',
      'push',
      'kakao',
      'naver',
      'google',
      'apple',
      'ai',
      'backup',
      'payout',
      'juso',
    ])
      setupCard(code, cardFields[code]!),
  ],
  'dev_mode': false,
  'public_base_url': 'https://api.example.co.kr',
  'webhook_url': webhookUrl,
  'toss_webhook_url': tossPayoutWebhookUrl,
  'toss_payments_webhook_url': tossPaymentsWebhookUrl,
  'payment_gateways': <Json>[],
  'payment_gateway': null,
};

/// Payment methods showing each row line the manual explains (10.9): card and KakaoPay on PortOne, bank transfer
/// and the virtual account on Toss Payments, and NaverPay with no gateway.
List<Json> payMethods() => [
  {'code': 'card', 'enabled': true, 'configured': true, 'gateway': 'portone'},
  {
    'code': 'kakaopay',
    'enabled': true,
    'configured': true,
    'gateway': 'portone',
  },
  {'code': 'naverpay', 'enabled': false, 'configured': false, 'gateway': null},
  {
    'code': 'transfer',
    'enabled': true,
    'configured': true,
    'gateway': 'tosspayments',
  },
  {
    'code': 'virtual_account',
    'enabled': false,
    'configured': true,
    'gateway': 'tosspayments',
  },
];

/// A business profile filled in the way an owner would.
Json sampleBusiness({String color = '#1E6FD9'}) => {
  'name': '말끔 청소',
  'support_phone': '1588-0000',
  'support_email': 'help@example.co.kr',
  'support_hours': '평일 09:00~18:00',
  'primary_color': color,
  'logo_url': null,
  'company': {
    'legal_name': '주식회사 예시',
    'ceo': '홍길동',
    'registration_no': '000-00-00000',
    'mail_order_no': '제2026-서울강남-0000호',
    'address': '서울특별시 강남구 테헤란로 000',
    'hosting_provider': 'Amazon Web Services',
  },
  'app_store_url': 'https://apps.apple.com/kr/app/id0000000000',
  'play_store_url':
      'https://play.google.com/store/apps/details?id=kr.co.example.customer',
  'partner_app_store_url': '',
  'partner_play_store_url': '',
  'app_versions': {
    'customer': {'min_app_version': '1.4.0', 'latest_app_version': '1.5.1'},
    'partner': {'min_app_version': '1.4.0', 'latest_app_version': '1.5.1'},
  },
};

/// Dashboard numbers for a small, busy business.
const dashboardJson = <String, Object?>{
  'today_jobs': 7,
  'open_quotes': 3,
  'unassigned': 2,
  'unread_threads': 4,
  'revenue_30d': 12840000,
  'bookings_30d': 96,
  'average_rating': 4.8,
  'referrals_rewarded': 5,
  'referrals_invited': 18,
  'by_service': {'bathroom': 31, 'aircon': 22, 'move_in': 14, 'kitchen': 9},
};

/// Start-up reads every console page needs (shell, dashboard, server gate). Routes a shot adds
/// before this run first, so they win.
void addBase(FakeApi f) {
  f
    ..reply('GET', '/setup', {'needs_admin': false})
    ..reply('GET', '/admin/dashboard', dashboardJson)
    ..reply('GET', '/admin/setup', freshSetup())
    ..reply('GET', '/admin/payment-methods', payMethods())
    ..reply('GET', '/admin/app-links', {
      'android_cert_sha256': <String>[],
      'apple_team_id': null,
      'source': 'env',
    });
  stockConsoleApi(f);
}
