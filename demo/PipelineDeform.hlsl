#include "ConstantBuffer.hlsl"
#include "VertexInput.hlsl"
#include "Utils.hlsl"

struct VS_OUTPUT
{
	float4 Pos : SV_POSITION;

	float4 SurfaceFromObject0 : TEXCOORD1;
	float4 SurfaceFromObject1 : TEXCOORD2;
	float4 SurfaceFromObject2 : TEXCOORD3;
};

VS_OUTPUT vs_main( VS_INPUT In )
{
	VS_OUTPUT Out;
	float4 posObject = float4( In.Pos, 1.0 );
	float4 normObject = float4( In.Normal, 0.0f );
	float4 tangentObject = float4( In.Tangent, 0.0f );
	float4 bitangentObject = float4( In.Bitangent, 0.0f );

	Animate ( posObject,
			  normObject,
			  tangentObject,
			  bitangentObject,
			  In.BoneWeights );

	// Note here we do NOT extrude along the normal.

	// Form the matrix that transforms a point from "surface" space (vertex u,v,height) to object space (vertex position x,y,z)
	// Note this is NOT orthonormal - it has scales, and tangent and bitangent are not guaranteed at right angles,
	// so the inverse is not just the transpose, which is why it's a Mat44 not an Orn.
	float4x4 objectFromSurface;
	// Note in (u,v,height), height=along the normal.
	objectFromSurface[0][0] = tangentObject.x;
	objectFromSurface[1][0] = tangentObject.y;
	objectFromSurface[2][0] = tangentObject.z;
	objectFromSurface[3][0] = 0.0f;
	objectFromSurface[0][1] = bitangentObject.x;
	objectFromSurface[1][1] = bitangentObject.y;
	objectFromSurface[2][1] = bitangentObject.z;
	objectFromSurface[3][1] = 0.0f;
	objectFromSurface[0][2] = normObject.x;
	objectFromSurface[1][2] = normObject.y;
	objectFromSurface[2][2] = normObject.z;
	objectFromSurface[3][2] = 0.0f;
	// Start with a position offset of zero.
	objectFromSurface[0][3] = 0.0f;
	objectFromSurface[1][3] = 0.0f;
	objectFromSurface[2][3] = 0.0f;
	objectFromSurface[3][3] = 1.0f;

	// So then if we transform the vertex's Surface Space coordinate...
	// (note the Z coordinate is 0.5, which is the middle of the heightfield)
	float4 surfacePos = float4 ( In.TexCoord.x, In.TexCoord.y, 0.5f, 1.0f );
	float4 resultObjectPos = mul ( objectFromSurface, surfacePos );
	// ...we want the result to be the vertex's object-space position.
	float4 desiredObjectPos = posObject;
	float4 posOffset = desiredObjectPos - resultObjectPos;
	objectFromSurface[0][3] = posOffset.x;
	objectFromSurface[1][3] = posOffset.y;
	objectFromSurface[2][3] = posOffset.z;
	objectFromSurface[3][3] = 1.0f;

	// ...but actually what we want is the opposite - to get from "real world" Object Space,
	// into Surface Space (u,v,height) so we can sample the heightfield.
	float4x4 surfaceFromObject = GetInverse4x4 ( objectFromSurface );

	Out.SurfaceFromObject0 = float4 ( surfaceFromObject._m00, surfaceFromObject._m01, surfaceFromObject._m02, surfaceFromObject._m03 );
	Out.SurfaceFromObject1 = float4 ( surfaceFromObject._m10, surfaceFromObject._m11, surfaceFromObject._m12, surfaceFromObject._m13 );
	Out.SurfaceFromObject2 = float4 ( surfaceFromObject._m20, surfaceFromObject._m21, surfaceFromObject._m22, surfaceFromObject._m23 );

	// The "position" output is the coordinates in surface space,
	// i.e. the UV coordinates, remapped from [0,1] to [-1,+1]
	Out.Pos.x = (In.TexCoord.x - 0.5f) * 2.0f;
	Out.Pos.y = (0.5f - In.TexCoord.y) * 2.0f;
	Out.Pos.z = 1.0f;
	Out.Pos.w = 1.0f;

	return Out;
}

SamplerState smp;

struct PS_OUTPUT
{
	float4 Rt0 : COLOR0;
	float4 Rt1 : COLOR1;
	float4 Rt2 : COLOR2;
};

PS_OUTPUT ps_main( VS_OUTPUT In ) : SV_TARGET
{
	PS_OUTPUT result;
	result.Rt0 = In.SurfaceFromObject0;
	result.Rt1 = In.SurfaceFromObject1;
	result.Rt2 = In.SurfaceFromObject2;

	return result;
}

