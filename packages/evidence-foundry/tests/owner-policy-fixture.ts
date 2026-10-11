import { Foundry as CanonicalFoundry } from "../src/service";
import { Store } from "../src/modules/storage/store";
export * from "../src/service";
// Synthetic current-policy fixture only; no semantic-owner authority or original checkout dependency.
const policy = { excludedDomains: ["ministry","spt","math-teaching-and-educational-video","music-production","hermes-application-workflows","jev-research-experiments"] };
export class Foundry extends CanonicalFoundry {
  constructor(store: Store) { super(store, policy); }
  static async open(root: string) { return new Foundry(await new Store(root).initialize()); }
}
