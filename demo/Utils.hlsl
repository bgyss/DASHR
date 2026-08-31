float4x4 GetInverse4x4 ( float4x4 mat )
{
	// Thanks to ccVector library for this inverse function.
	float s[6];
	float c[6];

	s[0] = mat[0][0] * mat[1][1] - mat[1][0] * mat[0][1];
	s[1] = mat[0][0] * mat[1][2] - mat[1][0] * mat[0][2];
	s[2] = mat[0][0] * mat[1][3] - mat[1][0] * mat[0][3];
	s[3] = mat[0][1] * mat[1][2] - mat[1][1] * mat[0][2];
	s[4] = mat[0][1] * mat[1][3] - mat[1][1] * mat[0][3];
	s[5] = mat[0][2] * mat[1][3] - mat[1][2] * mat[0][3];

	c[0] = mat[2][0] * mat[3][1] - mat[3][0] * mat[2][1];
	c[1] = mat[2][0] * mat[3][2] - mat[3][0] * mat[2][2];
	c[2] = mat[2][0] * mat[3][3] - mat[3][0] * mat[2][3];
	c[3] = mat[2][1] * mat[3][2] - mat[3][1] * mat[2][2];
	c[4] = mat[2][1] * mat[3][3] - mat[3][1] * mat[2][3];
	c[5] = mat[2][2] * mat[3][3] - mat[3][2] * mat[2][3];

	float det = s[0] * c[5] - s[1] * c[4] + s[2] * c[3] + s[3] * c[2] - s[4] * c[1] + s[5] * c[0];
	float idet = 1.0f / det;

	float4x4 result;

	result[0][0] = ( mat[1][1] * c[5] - mat[1][2] * c[4] + mat[1][3] * c[3]) * idet;
	result[0][1] = (-mat[0][1] * c[5] + mat[0][2] * c[4] - mat[0][3] * c[3]) * idet;
	result[0][2] = ( mat[3][1] * s[5] - mat[3][2] * s[4] + mat[3][3] * s[3]) * idet;
	result[0][3] = (-mat[2][1] * s[5] + mat[2][2] * s[4] - mat[2][3] * s[3]) * idet;

	result[1][0] = (-mat[1][0] * c[5] + mat[1][2] * c[2] - mat[1][3] * c[1]) * idet;
	result[1][1] = ( mat[0][0] * c[5] - mat[0][2] * c[2] + mat[0][3] * c[1]) * idet;
	result[1][2] = (-mat[3][0] * s[5] + mat[3][2] * s[2] - mat[3][3] * s[1]) * idet;
	result[1][3] = ( mat[2][0] * s[5] - mat[2][2] * s[2] + mat[2][3] * s[1]) * idet;

	result[2][0] = ( mat[1][0] * c[4] - mat[1][1] * c[2] + mat[1][3] * c[0]) * idet;
	result[2][1] = (-mat[0][0] * c[4] + mat[0][1] * c[2] - mat[0][3] * c[0]) * idet;
	result[2][2] = ( mat[3][0] * s[4] - mat[3][1] * s[2] + mat[3][3] * s[0]) * idet;
	result[2][3] = (-mat[2][0] * s[4] + mat[2][1] * s[2] - mat[2][3] * s[0]) * idet;

	result[3][0] = (-mat[1][0] * c[3] + mat[1][1] * c[1] - mat[1][2] * c[0]) * idet;
	result[3][1] = ( mat[0][0] * c[3] - mat[0][1] * c[1] + mat[0][2] * c[0]) * idet;
	result[3][2] = (-mat[3][0] * s[3] + mat[3][1] * s[1] - mat[3][2] * s[0]) * idet;
	result[3][3] = ( mat[2][0] * s[3] - mat[2][1] * s[1] + mat[2][2] * s[0]) * idet;

	return result;
}

void Animate ( inout float4 posObject,
			   inout float4 normObject,
			   inout float4 tangentObject,
			   inout float4 bitangentObject,
			   float4 boneWeights )
{
	// Animation. This is completely standard 4-bone skinning, except without bone indices.
	float4x4 boneFromObjectTotal;
	boneFromObjectTotal  = BoneFromObject[0] * boneWeights.x;
	boneFromObjectTotal += BoneFromObject[1] * boneWeights.y;
	boneFromObjectTotal += BoneFromObject[2] * boneWeights.z;
	boneFromObjectTotal += BoneFromObject[3] * boneWeights.w;
	
	float4 posObject2       = mul (boneFromObjectTotal, posObject      );
	float4 normObject2      = mul (boneFromObjectTotal, normObject     );
	float4 tangentObject2   = mul (boneFromObjectTotal, tangentObject  );
	float4 bitangentObject2 = mul (boneFromObjectTotal, bitangentObject);

	posObject       = posObject2      ;
	normObject      = normObject2     ;
	tangentObject   = tangentObject2  ;
	bitangentObject = bitangentObject2;
}
