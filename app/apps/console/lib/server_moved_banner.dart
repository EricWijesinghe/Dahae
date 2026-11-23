// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart' show KeepAllText;

import 'server_address.dart';
import 'state.dart';

/// The server's own address (`/v1/config.api_base`), when it is an https origin other than [current].
String? movedBase(Object? config, String current) {
  if (config is! Json) return null;
  final u = Uri.tryParse(config['api_base'] as String? ?? '');
  if (u == null || u.scheme != 'https' || u.host.isEmpty) return null;
  return u.origin == Uri.parse(current).origin ? null : u.origin;
}

/// On a typed-in server address (desktop, sideload), says when the server now lives elsewhere, after a
/// domain change. The old address keeps working as an alias, so this is advice, never a forced move.
class ServerMovedBanner extends StatefulWidget {
  const ServerMovedBanner({super.key, required this.child});
  final Widget child;

  @override
  State<ServerMovedBanner> createState() => _ServerMovedBannerState();
}

class _ServerMovedBannerState extends State<ServerMovedBanner> {
  String? _moved;
  bool _asked = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_asked || ServerChange.of(context) == null) return;
    _asked = true;
    final api = context.api;
    // Advice only: an offline or failed check shows nothing.
    api
        .get('/config')
        .then((c) {
          if (mounted) setState(() => _moved = movedBase(c, api.baseUrl));
        })
        .catchError((Object _) {});
  }

  @override
  Widget build(BuildContext context) {
    final moved = _moved, change = ServerChange.of(context);
    if (moved == null || change == null) return widget.child;
    return Column(
      children: [
        MaterialBanner(
          content: KeepAllText(context.t.cRbServerMoved(moved)),
          actions: [
            TextButton(
              onPressed: change,
              child: KeepAllText(context.t.cRbServerMovedAction),
            ),
          ],
        ),
        Expanded(child: widget.child),
      ],
    );
  }
}
