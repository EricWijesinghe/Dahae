// Store screenshots for the customer app. Excluded from normal runs (tag `store_screenshots`).
// Generate with `scripts/store/generate.sh` (see docs/guide/03-앱-출시.md). Writes PNGs under
// build/store_shots/raw/customer/<device>/<lang>/NN-name.png; scripts/store/frame.js adds captions.
@Tags(['store_screenshots'])
library;

import 'package:core/core.dart';
import 'package:customer/screens/booking_detail.dart';
import 'package:customer/screens/booking_flow.dart';
import 'package:customer/screens/chat.dart';
import 'package:customer/screens/home.dart';
import 'package:customer/screens/service.dart';
import 'package:customer/state.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import '../support/customer_host.dart';
import '../support/customer_routes.dart';

/// Rich, realistic data: Korean names, Seoul addresses, prices from the default catalogue.
/// Home copy for the store shots: real-looking Korean content, the same wording as the seeded demo.
/// Registered before the stock `/content` route, so the other customer tests keep their fixtures.
Json _content(
  String id,
  String kind,
  Map<String, String> title, {
  Json body = const {},
  String? link,
}) => {
  'id': id,
  'kind': kind,
  'title': title,
  'link': link,
  'image_url': null,
  'body': body,
  'created_at': null,
};

Object _contentFor(String? kind) => switch (kind) {
  'banner' => [
    _content(
      'bn-1',
      'banner',
      const {
        'ko': '첫 청소 15,000원 쿠폰 즉시 지급',
        'en': '₩15,000 coupon for your first clean',
      },
      body: const {
        'text': {'ko': '신규 고객 웰컴 쿠폰', 'en': 'Welcome coupon for new customers'},
        'card': {
          'label': {'ko': '웰컴 혜택', 'en': 'Welcome gift'},
          'value': {'ko': '15,000원', 'en': '₩15,000'},
        },
      },
      link: 'coupons',
    ),
    _content(
      'bn-2',
      'banner',
      const {
        'ko': '입주 청소, 이사 전 청소까지 한 번에',
        'en': 'Move-in cleaning, including the pre-move-out clean',
      },
      body: const {
        'text': {
          'ko': '나가는 집과 들어가는 집을 함께 맡기면 일정을 한 번에 맞춰 드려요.',
          'en':
              'Book both homes together and we line up the two visits for you.',
        },
      },
      link: 'service/move_in',
    ),
  ],
  'notice' => [
    _content(
      'nt-1',
      'notice',
      const {'ko': '연말 운영 시간 안내', 'en': 'Year-end opening hours'},
      body: const {
        'text': {
          'ko': '12월 31일은 오후 4시까지 상담을 받아요.',
          'en': 'We take consultations until 4 pm on December 31.',
        },
      },
    ),
  ],
  'news' => [
    _content(
      'nw-1',
      'news',
      const {
        'ko': '원룸 청소 서비스 지역 확대',
        'en': 'Studio cleaning now covers more areas',
      },
      body: const {
        'text': {
          'ko': '이번 달부터 성남시 분당구와 하남시에서도 원룸 청소를 예약할 수 있어요.',
          'en': 'Studio cleaning can now be booked in Bundang and Hanam.',
        },
      },
    ),
    _content(
      'nw-2',
      'news',
      const {
        'ko': '2in1 에어컨 청소 예약 오픈',
        'en': '2-in-1 aircon cleaning is open for booking',
      },
      body: const {
        'text': {
          'ko': '스탠드와 벽걸이를 한 번에 청소하는 2in1 상품을 예약할 수 있어요.',
          'en': 'Stand and wall units cleaned in one visit.',
        },
      },
    ),
    _content(
      'nw-3',
      'news',
      const {'ko': '입주 청소 묶음 예약', 'en': 'Bundled move-in bookings'},
      body: const {
        'text': {
          'ko': '나가는 집과 들어가는 집을 함께 예약하면 일정을 한 번에 맞춰 드려요.',
          'en': 'Book both homes together and we line up the visits.',
        },
      },
    ),
  ],
  'tip' => [
    _content(
      'tp-1',
      'tip',
      const {'ko': '에어컨 냄새의 원인은?', 'en': 'What causes aircon odours?'},
      body: const {
        'text': {
          'ko': '냉각핀에 남은 물기에 곰팡이가 자라면 쉰 냄새가 나요.',
          'en': 'Mould on damp cooling fins causes the sour smell.',
        },
      },
    ),
    _content(
      'tp-2',
      'tip',
      const {'ko': '욕실 물때 없애는 법', 'en': 'Removing bathroom limescale'},
      body: const {
        'text': {
          'ko': '식초 물을 뿌리고 10분 뒤 헹구면 대부분 지워져요.',
          'en': 'Spray diluted vinegar, wait ten minutes, then rinse.',
        },
      },
    ),
  ],
  _ => const <Json>[],
};

FakeApi _richApi() {
  final now = DateTime.now();
  final upcoming = bookingJson(
    id: 'bk-1',
    number: 'B-20261114-0042',
    serviceCode: 'bathroom',
    status: 2,
    visitDate: now.add(const Duration(days: 3)),
    startHour: 10,
    unitIndex: 1,
    minutes: 120,
    estimate: 52000,
    listPrice: 52000,
    finalPrice: 52000,
    paidAmount: 52000,
    address: addressJson(detail: '101동 1001호', sizePyeong: 32),
  );
  final done = bookingJson(
    id: 'bk-0',
    number: 'B-20261021-0007',
    serviceCode: 'aircon',
    status: 3,
    visitDate: now.subtract(const Duration(days: 20)),
    startHour: 14,
    unitIndex: 0,
    minutes: 90,
    listPrice: 79000,
    finalPrice: 79000,
    paidAmount: 79000,
  );
  final fake = FakeApi()
    ..onGet('/content', (call) => _contentFor(call.query['kind']));
  stockCustomerApi(
    fake,
    name: '지민',
    addresses: [
      addressJson(),
      addressJson(
        id: 'addr-2',
        kind: 'office',
        label: '사무실',
        roadAddress: '서울특별시 마포구 월드컵북로 400',
        detail: '5층 502호',
        districtCode: '11440',
        sizePyeong: 34,
      ),
    ],
    coupons: [couponJson(id: 'cp-1', name: '첫 청소 1만원 할인', amount: 10000)],
    methods: [
      paymentMethodJson(id: 'pm-1', provider: 'card', displayName: '신한 1234'),
      paymentMethodJson(id: 'pm-2', provider: 'kakaopay', displayName: '카카오페이'),
    ],
    bookings: [upcoming, done],
  );
  fake
    ..reply('GET', '/bookings/bk-1', upcoming)
    ..reply('GET', '/slots', openSlots())
    ..reply('POST', '/chat/threads/th-1/read', null)
    ..reply('GET', '/chat/threads/th-1/messages', [
      chatMessageJson(
        id: 'm1',
        sender: 'system',
        kind: 'system',
        body: {
          'code': 'deposit_paid',
          'params': {'amount': 15600},
        },
      ),
      chatMessageJson(
        id: 'm2',
        sender: 'staff',
        kind: 'text',
        body: {'text': '안녕하세요, 말끔 매니저 김도윤입니다. 내일 오전 10시에 방문 예정이에요.'},
      ),
      chatMessageJson(
        id: 'm3',
        sender: 'customer',
        kind: 'text',
        body: {'text': '욕실 줄눈 청소도 같이 가능한가요?'},
      ),
      chatMessageJson(
        id: 'm4',
        sender: 'staff',
        kind: 'text',
        body: {'text': '네, 줄눈 클리닝은 추가 옵션으로 가능해요. 견적 보내드릴게요.'},
      ),
      chatMessageJson(
        id: 'm5',
        sender: 'staff',
        kind: 'quote',
        body: {
          'booking_id': 'bk-1',
          'final_price': 52000,
          'deposit': 15600,
          'balance': 36400,
          'note': '줄눈 포함',
        },
      ),
    ]);
  return fake;
}

Future<TestEnv> _env(String lang, ThemeMode mode) =>
    TestEnv.create(fake: _richApi(), localeCode: lang, themeMode: mode);

/// One screen per store shot. [dark] picks the theme that reads best for that screen.
class _Shot {
  const _Shot(this.file, {required this.dark, required this.screen});
  final String file;
  final bool dark;
  final Widget Function() screen;
}

final _shots = <_Shot>[
  _Shot(
    '01-home',
    dark: false,
    screen: () => Scaffold(body: HomeTab(onOpenTab: (_) {})),
  ),
  _Shot(
    '02-service',
    dark: false,
    screen: () => ServiceScreen(
      service: Service.fromJson(serviceJson('bathroom')),
      initialUnit: 1,
    ),
  ),
  _Shot(
    '03-schedule',
    dark: false,
    screen: () => _draftScreen((d) => ScheduleScreen(draft: d)),
  ),
  _Shot(
    '04-checkout',
    dark: false,
    screen: () => _draftScreen((d) => CheckoutScreen(draft: d)),
  ),
  _Shot(
    '05-tracking',
    dark: true,
    screen: () => const BookingDetailScreen(id: 'bk-1'),
  ),
  _Shot(
    '06-chat',
    dark: true,
    screen: () => const ChatRoomScreen(
      threadId: 'th-1',
      topic: 'bathroom',
      bookingId: 'bk-1',
    ),
  ),
];

/// A booking draft filled the way a user leaves it at the schedule step.
Widget _draftScreen(Widget Function(BookingDraft draft) build) {
  final draft = BookingDraft(Service.fromJson(serviceJson('bathroom')))
    ..unitIndex = 1
    ..address = Address.fromJson(addressJson())
    ..date = DateUtils.dateOnly(DateTime.now().add(const Duration(days: 3)))
    ..hour = 10
    ..methodId = 'pm-1';
  return build(draft);
}

void main() {
  for (final lang in shotLangs) {
    testWidgets('store shots (customer, $lang)', (tester) async {
      await tester.runAsync(loadShotFonts);
      final envs = {
        for (final mode in const [ThemeMode.light, ThemeMode.dark])
          mode: await _env(lang, mode),
      };
      final devices = shotDevices.where(
        (d) => shotDeviceSlugs.contains(d.slug),
      );
      for (final device in devices) {
        for (final shot in _shots) {
          final env = envs[shot.dark ? ThemeMode.dark : ThemeMode.light]!;
          final app = AppState(env.api, FakeRealtime(env.api));
          // The Shell loads these on start in the real app; screens shot alone need them up front.
          await tester.runAsync(app.loadAll);
          await captureShot(
            tester,
            CustomerTestHost(env: env, app: app, home: shot.screen()),
            device,
            '$shotOutRoot/customer/${device.slug}/$lang/${shot.file}.png',
          );
          await unmount(tester);
        }
      }
    });
  }
}
