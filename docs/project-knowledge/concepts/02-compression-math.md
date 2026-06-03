# 2. Mathematics for Compression

No need to go full academic, but the fundamentals matter.

The most important idea:

```text
Compression is about representing predictable data with fewer bits.
```

## 2.1 Entropy

Entropy measures uncertainty or information content.

Formula:

```text
H(X) = -sum(p_i * log2(p_i))
```

Meaning:

- The more predictable the data, the lower the entropy.
- Lower entropy usually means better compression potential.
- High-entropy data is hard or impossible to compress well.

Examples:

```text

AAAAAAAAAAAA
```

Very predictable. Compresses well.

```text
x8J!2Lm#Qp9@
```

Less predictable. Harder to compress.

Encrypted or random data usually has high entropy.

### Entropy intuition

If a symbol always appears, it carries little new information.

If many symbols appear with equal probability, each symbol carries more information.

Example:

```text
A A A A A A A A
```

A compressor can encode this with a short representation.

Example:

```text
A X 7 Q M 2 Z !
```

There is less pattern to exploit.

## 2.2 Probability Distribution

Compression depends heavily on symbol probability.

Must understand:

- Frequency tables.
- Histograms.
- Symbol probability.
- Adaptive probability.

This is the foundation for:

- Huffman coding.
- Arithmetic coding.
- ANS.
- Range coding.

### Frequency table

Example input:

```text
AAABBCCCC

```

Frequency table:

```text
A: 3
B: 2
C: 4
```

Probability:

```text
A: 3/9

B: 2/9
C: 4/9
```

Huffman coding uses these frequencies to assign shorter codes to more frequent symbols.

## 2.3 Prefix Codes

A prefix code is a code where no code is the prefix of another code.

Good:

```text
A = 0
B = 10
C = 11
```

Bad:

```text

A = 0
B = 01
```

Because `0` is a prefix of `01`, decoding becomes ambiguous.

Huffman coding creates prefix codes.

## 2.4 Entropy vs Compression Ratio

Entropy gives a theoretical lower bound.

Compression ratio is the practical result.

```text
compression ratio = compressed_size / original_size
```

Example:

```text
original   = 1000 bytes
compressed = 250 bytes

ratio      = 0.25
```

Or as reduction:

```text
space saved = 75%
```

Real compressors cannot always reach entropy limits due to:

- Metadata overhead.
- Block boundaries.
- Simple models.
- Speed tradeoffs.
- Format constraints.

## 2.5 Modeling

A compressor has a model of the data.

Examples:

- RLE model: repeated adjacent symbols are common.
- Delta model: current number is close to previous number.
- LZ77 model: repeated byte sequences occur nearby.
- Huffman model: some symbols appear more often than others.

Better model means better compression potential.

Wrong model can make data larger.

Example:

```text
Apply delta to random text bytes
```

This is usually a bad model.

---

