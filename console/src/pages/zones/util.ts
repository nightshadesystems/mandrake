// Validation shared by the zone wizard and the edit dialog.

import type { ZoneNic } from '../../api/zones.ts';

const LINK_NAME = /^[a-zA-Z][a-zA-Z0-9_]{0,30}[0-9]$/;

export function nicErrors(nics: ZoneNic[]): string[] {
  const errors: string[] = [];
  const seen = new Set<string>();
  nics.forEach((n) => {
    if (!LINK_NAME.test(n.name)) errors.push(`NIC name "${n.name}" must end in a digit`);
    if (seen.has(n.name)) errors.push(`NIC "${n.name}" is listed twice`);
    seen.add(n.name);
    if (!n.over) errors.push(`NIC "${n.name}" needs a link beneath it`);
    if (n.vid !== undefined && (n.vid < 1 || n.vid > 4094)) {
      errors.push(`NIC "${n.name}": VLAN id must be 1 to 4094`);
    }
    if (n.address && !/^[0-9a-fA-F:.]+\/\d{1,3}$/.test(n.address)) {
      errors.push(`NIC "${n.name}": address must include a prefix length`);
    }
  });
  return errors;
}
