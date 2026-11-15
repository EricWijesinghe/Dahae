import 'package:core/core.dart';
import 'package:customer/screens/move_chooser.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/customer_host.dart';

ServiceGroup _group(Json config) => ServiceGroup.fromJson({
  'code': 'moving',
  'title': {'ko': '이사·입주 청소', 'en': 'Move cleaning'},
  'sort_order': 0,
  'active': true,
  'chooser': 'move',
  'icon': 'svc_move',
  'badge': null,
  'services': ['move_in', 'move_out'],
  'config': config,
});

void main() {
  test('server config drives the home types and build-age services (CR-8.1)', () {
    final config = {
      'build_age': {'new': 'move_out', 'old': 'move_in'},
      'home_types': ['apartment', 'bogus'],
    };
    expect(moveHomesFor(config), [MoveHome.apartment]);
    expect(buildAgeServiceCode(config, isNew: true), 'move_out');
    expect(buildAgeServiceCode(config, isNew: false), 'move_in');
  });

  test('a group without config keeps the built-in homes and services', () {
    expect(moveHomesFor(const {}), MoveHome.values);
    expect(buildAgeServiceCode(const {}, isNew: true), 'move_in');
    expect(buildAgeServiceCode(const {}, isNew: false), 'move_out');
  });

  testWidgets('the chooser offers only the home types the server sends', (
    tester,
  ) async {
    final env = await TestEnv.create(fake: FakeApi());
    await pumpCustomerScreen(
      tester,
      env,
      MoveChooserScreen(
        group: _group({
          'home_types': ['apartment'],
        }),
      ),
    );
    await settle(tester);
    await tester.tap(find.text(koT.hgNewBuild));
    await settle(tester);

    expect(find.text(koT.hgHomeApartment), findsOneWidget);
    expect(find.text(koT.hgHomeOneTwo), findsNothing);
    expect(find.text(koT.hgHomeVilla), findsNothing);
    await unmount(tester);
  });
}
