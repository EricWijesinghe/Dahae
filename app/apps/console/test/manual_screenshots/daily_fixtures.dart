// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

// Sample data for the daily-work chapters (catalog, bookings, people, money, marketing, rules).
// Every name, phone number and account is made up.

import 'package:core/core.dart';
import 'package:test_support/test_support.dart';

import '../support/payout_fixtures.dart';

final _today = DateTime.now();
String _iso(DateTime d) => d.toUtc().toIso8601String();

/// Catalogue as the admin list sends it (default prices, all active).
List<Json> adminServices() => [
  for (final c in [...catalogCodes, 'whole_home', 'window', 'disinfection'])
    {...serviceJson(c), 'active': true, 'sort_order': 0},
];

/// The four groups the server seeds (migrations/0234_service_groups.sql).
Json serviceGroups() => {
  'groups': [
    _group('moving', '이사·입주 청소', 'Move-in / move-out cleaning', 'move', [
      'move_in',
      'move_out',
      'studio',
    ]),
    _group('living', '거주 청소', 'Living-space cleaning', 'list', [
      'bathroom',
      'kitchen',
      'whole_home',
      'window',
    ]),
    _group(
      'appliance',
      '가전·가구 청소',
      'Appliance and furniture cleaning',
      'list',
      ['aircon', 'mattress', 'sofa'],
    ),
    _group('disinfection', '방역·소독', 'Pest control and disinfection', 'direct', [
      'disinfection',
    ]),
  ],
};

Json _group(
  String code,
  String ko,
  String en,
  String chooser,
  List<String> services,
) => {
  'code': code,
  'title': {'ko': ko, 'en': en},
  'sort_order': 0,
  'active': true,
  'icon': 'svc_$code',
  'chooser': chooser,
  'services': services,
  'config': chooser == 'move'
      ? {
          'build_age': {'new': 'move_in', 'old': 'move_out'},
          'home_types': ['apartment'],
        }
      : <String, Object?>{},
};

Json airconCounts() => {
  'type': 'counts',
  'units': [
    {'label_key': 'wall', 'label': '벽걸이', 'minutes': 90, 'price': 79000},
    {'label_key': 'stand', 'label': '스탠드', 'minutes': 120, 'price': 119000},
    {
      'label_key': 'two_in_one',
      'label': '2in1',
      'minutes': 180,
      'price': 179000,
    },
  ],
  'max_each': 5,
  'multi_discount_bps': 1000,
};

Json quoteBooking() => bookingJson(
  id: 'bk-q',
  number: 'B-20261121-0031',
  serviceCode: 'move_in',
  status: 0,
  visitDate: _today.add(const Duration(days: 9)),
  startHour: 9,
  estimate: 420000,
  answers: {'cleaning_type': 'standard', 'luggage': 'empty'},
  address: addressJson(detail: '203동 1502호', sizePyeong: 34),
);

Json confirmedBooking() => bookingJson(
  id: 'bk-1',
  number: 'B-20261121-0042',
  serviceCode: 'bathroom',
  status: 2,
  visitDate: _today.add(const Duration(days: 2)),
  startHour: 10,
  unitIndex: 1,
  listPrice: 52000,
  finalPrice: 52000,
  deposit: 15600,
  paidAmount: 15600,
  address: addressJson(detail: '101동 1001호', sizePyeong: 32),
);

List<Json> bookingRows() => [
  confirmedBooking(),
  bookingJson(
    id: 'bk-2',
    number: 'B-20261120-0017',
    serviceCode: 'aircon',
    status: 1,
    visitDate: _today.add(const Duration(days: 4)),
    startHour: 14,
    unitIndex: 0,
    listPrice: 79000,
  ),
  quoteBooking(),
  bookingJson(
    id: 'bk-3',
    number: 'B-20261118-0009',
    serviceCode: 'kitchen',
    status: 3,
    visitDate: _today.subtract(const Duration(days: 3)),
    startHour: 13,
    listPrice: 45000,
    finalPrice: 45000,
    paidAmount: 45000,
  ),
];

Json detailOf(Json booking, {Json? estimate}) => {
  'booking': booking,
  'customer': {'name': '김지민', 'phone': '01012345678', 'email': null},
  'payments': <Json>[],
  'estimate_breakdown': estimate,
};

/// Server estimate for the sample move-in request (34 pyeong, standard clean).
const quoteEstimate = <String, Object?>{
  'base': 381818,
  'extras': 0,
  'surcharge_bps': 0,
  'before_vat': 381818,
  'total': 420000,
};

List<Json> threads() => [
  threadJson(
    id: 'th-1',
    topic: 'bathroom',
    bookingId: 'bk-1',
    unreadStaff: 2,
    lastMessage: {
      'kind': 'text',
      'body': {'text': '욕실 줄눈 청소도 같이 가능한가요?'},
      'sender': 'customer',
    },
  ),
  threadJson(
    id: 'th-2',
    topic: 'aircon',
    lastMessage: {
      'kind': 'text',
      'body': {'text': '벽걸이 2대 가격이 궁금해요'},
      'sender': 'customer',
    },
  ),
];

List<Json> chatMessages({bool ai = false}) => [
  chatMessageJson(
    id: 'm1',
    sender: 'customer',
    kind: 'text',
    body: {'text': '안녕하세요, 욕실 2개 청소 예약했어요.'},
  ),
  chatMessageJson(
    id: 'm2',
    sender: 'staff',
    kind: 'text',
    body: {'text': '안녕하세요! 예약 확인했어요. 방문 전날 다시 안내드릴게요.'},
  ),
  chatMessageJson(
    id: 'm3',
    sender: 'customer',
    kind: 'text',
    body: {'text': '욕실 줄눈 청소도 같이 가능한가요?'},
  ),
  if (ai)
    chatMessageJson(
      id: 'm4',
      kind: 'text',
      body: {'text': '네, 줄눈 청소는 추가 옵션으로 가능해요. 담당자가 견적을 보내드릴게요.', 'ai': true},
    ),
];

Json subscription(String id, String service, int weekday, String status) => {
  'id': id,
  'user_id': 'u-1',
  'service_code': service,
  'unit_index': null,
  'address_id': 'addr-1',
  'payment_method_id': 'pm-1',
  'preferred_pro_id': null,
  'cadence_weeks': 2,
  'weekday': weekday,
  'start_hour': 10,
  'anchor_date': '2026-11-17',
  'status': status,
  'paused_reason': null,
  'discount_percent': 10,
  'consecutive_failures': 0,
  'created_at': '2026-11-01T00:00:00Z',
  'cancelled_at': null,
};

Json _pro(
  String id,
  String team,
  String name,
  String status, {
  int completed = 0,
  double? rating,
}) => {
  'id': id,
  'team_name': team,
  'name': name,
  'status': status,
  'phone': '0101234${id.hashCode.abs() % 9000 + 1000}',
  'completed': completed,
  'cancelled': 0,
  'rating': rating,
  'fee_bps': 2000,
  'quality_score': rating == null ? null : 92,
  'quality_status': rating == null ? null : 'ok',
};

List<Json> pros() => [
  _pro('pro-1', '민수 청소팀', '김민수', 'verified', completed: 128, rating: 4.9),
  _pro('pro-2', '하늘 클린', '이하늘', 'verified', completed: 64, rating: 4.7),
  _pro('pro-3', '새봄 청소', '박새봄', 'pending'),
  _pro('pro-4', '반짝 홈케어', '최반짝', 'suspended', completed: 12, rating: 3.9),
];

Json proProfile() => {
  'status': 'pending',
  'items': ['identity', 'tax_type', 'experience', 'emergency_contact'],
  'checklist': {'identity': true, 'tax_type': true},
  'decided_at': null,
  'profile': {
    'bio': '입주 청소 5년 경력',
    'emergency_contact': '010-0000-0000',
    'employment_type': 'freelance',
    'hired_on': '2026-11-01',
    'photo_url': null,
  },
};

List<Json> customers() => [
  for (final (i, name) in ['김지민', '이서준', '박하은', '최도윤'].indexed)
    {
      'id': 'u-$i',
      'name': name,
      'phone': '0109876${5430 + i}',
      'created_at': '2026-10-${10 + i}T03:00:00Z',
      'bookings': 4 - i,
    },
];

List<Json> staff() => [
  {
    'id': 'st-1',
    'name': '홍길동',
    'email': 'owner@example.co.kr',
    'role': 'admin',
    'active': true,
    'invited': false,
    'is_owner': true,
  },
  {
    'id': 'st-2',
    'name': '김상담',
    'email': 'cs@example.co.kr',
    'role': 'consultant',
    'active': true,
    'invited': false,
    'is_owner': false,
  },
  {
    'id': 'st-3',
    'name': '이운영',
    'email': 'ops@example.co.kr',
    'role': 'admin',
    'active': false,
    'invited': true,
    'is_owner': false,
  },
];

List<Json> settlements() => [
  for (final (i, s) in ['pending', 'held', 'paid'].indexed)
    {
      'id': 'st-$i',
      'team_name': ['민수 청소팀', '하늘 클린', '반짝 홈케어'][i],
      'number': 'B-2026111$i-00${21 + i}',
      'net': [41600, 63200, 35200][i],
      'payout_date': '2026-11-2${5 + i}',
      'state': s,
    },
];

/// Payout rows: manual ones plus Toss ones in each state the tab offers buttons for.
List<Json> payouts({bool toss = false}) => [
  if (!toss) ...payoutRows,
  if (toss) ...[
    {...payoutJson('po-5', 'requested'), 'provider': 'toss'},
    {...payoutJson('po-6', 'sent'), 'provider': 'toss'},
    {
      ...payoutJson('po-7', 'failed', failure: 'account_closed'),
      'provider': 'toss',
    },
  ],
];

Json financeSummary() => {
  'gross': 12840000,
  'net_revenue': 10272000,
  'platform_fee': 2568000,
  'vat_estimate': 1167000,
  'avg_order_value': 133750,
  'cancellation_bps': 420,
  'payouts': {'paid': 8100000, 'pending': 1240000},
  'recurring': {
    'mrr': 1380000,
    'churn_bps': 310,
    'active': 23,
    'recognized': 1290000,
  },
  'payment_methods': [
    {'method': 'card', 'amount': 9100000},
    {'method': 'kakaopay', 'amount': 2400000},
    {'method': 'naverpay', 'amount': 1340000},
  ],
};

List<Json> financeSeries() => [
  for (var i = 29; i >= 0; i--)
    {
      'bucket': _iso(_today.subtract(Duration(days: i))).substring(0, 10),
      'gross': 300000 + (i * 37 % 11) * 40000,
      'net_revenue': 240000 + (i * 37 % 11) * 32000,
      'platform_fee': 60000 + (i * 37 % 11) * 8000,
      'payouts': {'paid': 200000, 'pending': 0},
    },
];

List<Json> financePartners() => [
  {
    'name': '김민수',
    'team_name': '민수 청소팀',
    'jobs': 41,
    'settled_gross': 4100000,
    'fee': 820000,
    'net': 3280000,
    'paid': 2900000,
    'pending': 380000,
  },
  {
    'name': '이하늘',
    'team_name': '하늘 클린',
    'jobs': 27,
    'settled_gross': 2600000,
    'fee': 520000,
    'net': 2080000,
    'paid': 2080000,
    'pending': 0,
  },
];

List<Json> financeServices() => [
  {'service_code': 'bathroom', 'gross': 4200000},
  {'service_code': 'aircon', 'gross': 3600000},
  {'service_code': 'move_in', 'gross': 3100000},
  {'service_code': 'kitchen', 'gross': 1940000},
];

/// The business group of operating rules, values as the server resolves them (settings_registry.rs).
Json businessRules() => {
  'items': [
    _rule('upload_daily_quota', 100, 1, 10000, 'default'),
    _rule('max_open_quote_requests', 5, 1, 1000, 'db', def: 3),
    _rule('max_threads_per_day', 10, 1, 1000, 'default'),
    _rule('review_report_threshold', 3, 1, 50, 'default', tighten: 'up'),
    _rule('noshow_grace_minutes', 30, 10, 240, 'env'),
    _rule('ai_chat_retention_days', 180, 30, 3650, 'default'),
    _rule('upload_orphan_days', 30, 7, 365, 'default'),
    {
      ..._rule('reconcile_interval_secs', 60, 10, 3600, 'default'),
      'read_only': true,
      'applies': 'rebuild',
    },
  ],
};

Json _rule(
  String key,
  int value,
  int min,
  int max,
  String source, {
  int? def,
  String? tighten,
}) => {
  'key': key,
  'type': 'int',
  'value': value,
  'default': def ?? value,
  'tighten': tighten,
  'applies': 'instant',
  'read_only': false,
  'source': source,
  'min': min,
  'max': max,
};

Json notifyTemplates() => {
  'items': [
    for (final (kind, label, ko, en) in const [
      ('booking.created', '새 예약 (직원)', '새 예약', 'New booking'),
      ('booking.quote_requested', '견적 요청 (직원)', '견적 요청', 'New quote request'),
    ])
      {
        'kind': kind,
        'label': label,
        'variables': ['예약번호'],
        'ko': {
          'default': {'title': ko, 'body': '{예약번호}'},
          'custom': null,
        },
        'en': {
          'default': {'title': en, 'body': '{예약번호}'},
          'custom': null,
        },
      },
  ],
  'variables': ['예약번호', '업체명'],
};

List<Json> auditRows() => [
  {
    'id': 3,
    'actor_role': 'staff',
    'actor_id': 'st-1',
    'actor_name': '홍길동',
    'action': 'setup.update',
    'target': 'portone',
    'data_text': '{"fields":["store_id","api_secret"]}',
    'created_at': _iso(_today.subtract(const Duration(hours: 2))),
  },
  {
    'id': 2,
    'actor_role': 'staff',
    'actor_id': 'st-2',
    'actor_name': '김상담',
    'action': 'case.refund',
    'target': 'case-1',
    'data_text': '{"amount":5000}',
    'created_at': _iso(_today.subtract(const Duration(hours: 5))),
  },
  {
    'id': 1,
    'actor_role': 'system',
    'actor_id': null,
    'actor_name': null,
    'action': 'payout.sync',
    'target': 'payout-9',
    'data_text': '',
    'created_at': _iso(_today.subtract(const Duration(days: 1))),
  },
];

Json renewals() => {
  'items': [
    {
      'id': 'rn-1',
      'kind': 'domain',
      'name': 'example.co.kr',
      'due_on': _iso(_today.add(const Duration(days: 21))).substring(0, 10),
      'days_left': 21,
    },
    {
      'id': 'rn-2',
      'kind': 'hosting',
      'name': 'AWS EC2',
      'due_on': _iso(_today.add(const Duration(days: 9))).substring(0, 10),
      'days_left': 9,
    },
    {
      'id': 'rn-3',
      'kind': 'apple',
      'name': 'Apple Developer Program',
      'due_on': _iso(_today.add(const Duration(days: 140))).substring(0, 10),
      'days_left': 140,
    },
  ],
};
