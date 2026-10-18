/** Only independent package tests in the disposable source QA tree run concurrently.
 * Installs, generators, typechecking and fresh-process provider probes stay serial.
 */
export async function runIndependentPackageTests(
  checks: readonly (() => Promise<void>)[],
  concurrency: 1 | 2 = 2,
): Promise<void> {
  if (concurrency !== 1 && concurrency !== 2)
    throw Error("Source package tests permit only one or two concurrent checks.");
  let next = 0;
  let failure: { error: unknown } | undefined;
  const worker = async (): Promise<void> => {
    while (!failure) {
      const check = checks[next++];
      if (!check) return;
      try {
        await check();
      } catch (error) {
        failure ??= { error };
      }
    }
  };
  // Every admitted child drains before either failure or subsequent source probes.
  await Promise.all(Array.from({ length: Math.min(concurrency, checks.length) }, worker));
  if (failure) throw failure.error;
}
