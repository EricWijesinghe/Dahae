import 'package:core/core.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

void main() {
  test('window and mattress options show their names, never raw keys', () {
    expect(koT.unit('window_small'), koT.unit_window_small);
    expect(koT.unit('window_large'), koT.unit_window_large);
    expect(koT.unitShort('window_small'), koT.unitShort_small);
    expect(koT.unitShort('window_large'), koT.unitShort_large);
    expect(koT.unit('mattress'), koT.unit_mattress);
    expect(koT.unitShort('mattress'), koT.unitShort_mattress);
    expect(koT.unit('window_small'), isNot('window_small'));
    expect(koT.unit('mattress'), isNot('mattress'));
  });

  test('whole-home and the window and disinfection services have names', () {
    expect(koT.service('whole_home'), koT.svc_whole_home);
    expect(koT.serviceShort('whole_home'), koT.svcShort_whole_home);
    expect(koT.serviceShort('window'), koT.svcShort_window);
    expect(koT.serviceShort('disinfection'), koT.svcShort_disinfection);
    expect(koT.service('whole_home'), isNot('whole_home'));
  });
}
