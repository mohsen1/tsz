interface Promise<T> {
    finally(onfinally?: (() => void) | undefined | null): Promise<T>;
}
