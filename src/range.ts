
const result = range([50,41, 2, 4, 0, 1, 3, 40, 75, -1]);
console.log(result);

function range(numbers: number[]): string {
  return numbers.sort().reduce((result: number[][], value) => {
    if (!result.length) {
      result.push([value]);
    } else {
      const current: number[] = result[result.length - 1];
      const prev = current.length === 1 ? current[0] : current[1];
      if (value - prev === 1) {
        current[1] = value;
      } else {
        result.push([value]);
      }
    }
    return result;
  }, []).map(r => r.join('-')).join(',');
}