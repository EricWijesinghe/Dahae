import 'package:console/booking_labels.dart';
import 'package:console/screens/bookings.dart';
import 'package:core/core.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

const _row = 'B-20260101-0001';

/// Window service with two counts options, as `/admin/services` returns it.
final _windowService = <String, dynamic>{
  'code': 'window',
  'pricing': {
    'type': 'counts',
    'units': [
      {'label_key': 'small', 'minutes': 15, 'price': 9000},
      {'label_key': 'large', 'minutes': 25, 'price': 15000},
    ],
    'max_each': null,
    'multi_discount_bps': 1000,
  },
  'needs_preparation': false,
  'content': <String, dynamic>{},
};

Json _windowBooking({required List<int> counts}) => {
  ...bookingJson(id: 'bk-1', number: _row, serviceCode: 'window', status: 2),
  'unit_counts': counts,
};

void main() {
  test('counts line names each picked option in catalogue order', () {
    final b = Booking.fromJson(_windowBooking(counts: [3, 2]));
    expect(
      bookingCountsLine(koT, b, [_windowService]),
      '${koT.unitShort('small')} 3 · ${koT.unitShort('large')} 2',
    );
  });

  test(
    'no counts line for a booking without counts or without its service',
    () {
      final plain = Booking.fromJson(_windowBooking(counts: const []));
      expect(bookingCountsLine(koT, plain, [_windowService]), isNull);
      final counted = Booking.fromJson(_windowBooking(counts: [1, 0]));
      expect(bookingCountsLine(koT, counted, const []), isNull);
    },
  );

  testWidgets('booking detail shows the counts line for a counts booking', (
    tester,
  ) async {
    setViewport(tester, wideSize);
    final detail = bookingDetail(status: 2);
    detail['booking'] = _windowBooking(counts: [3, 2]);
    final fake = FakeApi()
      ..reply('GET', '/admin/bookings', [
        _windowBooking(counts: [3, 2]),
      ])
      ..onGet('/admin/bookings/bk-1', (_) => detail)
      ..reply('GET', '/admin/services', [_windowService]);
    stockConsoleApi(fake);
    final env = await TestEnv.create(fake: fake, role: 'staff_admin');
    await pumpConsoleScreen(tester, env, const BookingsScreen());
    await pumpUntilFound(
      tester,
      find.text('$_row · ${koT.serviceShort('window')}'),
    );
    await tester.tap(find.text('$_row · ${koT.serviceShort('window')}'));
    await settle(tester);

    expect(
      find.text('${koT.unitShort('small')} 3 · ${koT.unitShort('large')} 2'),
      findsOneWidget,
    );
    await unmount(tester);
  });
}
